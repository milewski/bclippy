//! Compiler-independent source discovery, diagnostics, and padding fixes.

use std::collections::BTreeSet;
use std::ffi::{OsStr, OsString};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use bclippy::padding::{Padding, RustSource};

/// Whether the padding command reports violations or writes their fixes.
pub(crate) enum FormatMode {
    /// Reports missing padding and exits unsuccessfully when any is found.
    Check,
    /// Writes whitespace-only fixes after all selected sources have parsed.
    Fix,
}

/// Owns an explicit padding operation and its source-selection arguments.
pub(crate) struct FormatCommand {
    /// The requested operation, independent of upstream Clippy's options.
    mode: FormatMode,
    /// Files or directories to inspect; an empty list selects the current directory.
    paths: Vec<PathBuf>,
}

impl FormatCommand {
    /// Validates file-selection arguments, using `--` to accept dash-prefixed paths.
    ///
    /// # Errors
    /// Rejects unknown options instead of silently treating them as file paths.
    pub(crate) fn new(mode: FormatMode, arguments: &[OsString]) -> io::Result<Self> {
        let mut literal_paths = false;
        let mut paths = Vec::new();

        for argument in arguments {
            if !literal_paths && argument == "--" {
                literal_paths = true;
            } else if !literal_paths && (argument == "--check" || argument == "--fix") {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "choose either --check or --fix; the operations cannot be combined or repeated",
                ));
            } else if !literal_paths && argument.as_encoded_bytes().starts_with(b"-") {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!(
                        "unknown padding option '{}'; provide Rust file/directory paths or use -- before a path",
                        argument.display()
                    ),
                ));
            } else {
                paths.push(PathBuf::from(argument));
            }
        }

        if paths.is_empty() {
            paths.push(PathBuf::from("."));
        }

        Ok(Self { mode, paths })
    }

    /// Checks selected sources or writes fixes, returning zero only when they comply.
    ///
    /// Every source is parsed before the first write. Symlinks and common generated,
    /// dependency, build, and hidden directories are not traversed. Parse and filesystem
    /// failures remain failures; an empty selection is not reported as a successful check.
    ///
    /// # Errors
    /// Returns contextual discovery, source parsing, concurrent-edit, or write errors.
    pub(crate) fn run(&self) -> io::Result<i32> {
        let mut paths = BTreeSet::new();

        for path in &self.paths {
            collect_sources(path, &mut paths)?;
        }

        if paths.is_empty() {
            return Err(io::Error::new(io::ErrorKind::NotFound, "no Rust source files selected"));
        }

        let checked_count = paths.len();
        let mut changes = Vec::new();

        for path in paths {
            let original = fs::read_to_string(&path).map_err(|error| path_error(&path, error))?;

            let syntax = RustSource::parse(&original).map_err(|error| {
                let location = error.span().start();
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("{}:{}:{}: {error}", path.display(), location.line, location.column + 1),
                )
            })?;

            let padding = syntax.padded();

            if !padding.lines().is_empty() {
                changes.push(PreparedFile { path, original, padding });
            }
        }

        for change in &changes {
            match self.mode {
                FormatMode::Check => {
                    for line in change.padding.lines() {
                        eprintln!(
                            "{}:{line}:1: incorrect blank-line padding around or inside a multiline block [bclippy::block_padding]",
                            change.path.display()
                        );
                    }
                }

                FormatMode::Fix => change.write()?,
            }
        }

        match self.mode {
            FormatMode::Check => {
                eprintln!(
                    "bclippy: checked {checked_count} Rust files; {} files need block padding",
                    changes.len()
                );
                Ok(i32::from(!changes.is_empty()))
            }

            FormatMode::Fix => {
                eprintln!("bclippy: checked {checked_count} Rust files; fixed {} files", changes.len());
                Ok(0)
            }
        }
    }
}

/// A parsed source replacement retained until every selected file has passed parsing.
struct PreparedFile {
    /// Canonical source path, deduplicated across overlapping selections.
    path: PathBuf,
    /// Bytes read during planning, used to refuse overwriting a later source edit.
    original: String,
    /// Whitespace-only replacement and diagnostic lines.
    padding: Padding,
}

impl PreparedFile {
    /// Writes an already validated fix while retaining the source's file permissions.
    ///
    /// # Errors
    /// Returns an error if the file changed since planning or cannot be read or written.
    fn write(&self) -> io::Result<()> {
        if fs::read_to_string(&self.path).map_err(|error| path_error(&self.path, error))? != self.original {
            return Err(io::Error::other(format!(
                "{} changed while preparing fixes; retry the command",
                self.path.display()
            )));
        }

        fs::write(&self.path, self.padding.source()).map_err(|error| path_error(&self.path, error))?;
        eprintln!("Fixed {} ({} blank lines)", self.path.display(), self.padding.lines().len());
        Ok(())
    }
}

/// Recursively selects Rust files without following symlinks or dependency/build trees.
///
/// # Errors
/// Returns the actual metadata, directory-read, or canonicalization failure with its path.
fn collect_sources(path: &Path, paths: &mut BTreeSet<PathBuf>) -> io::Result<()> {
    let metadata = fs::symlink_metadata(path).map_err(|error| path_error(path, error))?;

    if metadata.is_file() && path.extension() == Some(OsStr::new("rs")) {
        paths.insert(path.canonicalize().map_err(|error| path_error(path, error))?);
    } else if metadata.is_dir() && !excluded_directory(path) {
        for entry in fs::read_dir(path).map_err(|error| path_error(path, error))? {
            let entry = entry.map_err(|error| path_error(path, error))?;
            collect_sources(&entry.path(), paths)?;
        }
    }

    Ok(())
}

/// Identifies directories outside maintained Rust source without inspecting their contents.
fn excluded_directory(path: &Path) -> bool {
    path.file_name().is_some_and(|name| {
        (name.as_encoded_bytes().starts_with(b".") && name != ".")
            || ["target", "vendor", "node_modules"].iter().any(|excluded| name == *excluded)
    })
}

/// Adds the affected path while retaining the operating system error as the cause.
fn path_error(path: &Path, error: io::Error) -> io::Error {
    io::Error::new(
        error.kind(),
        PathError {
            path: path.to_owned(),
            cause: error,
        },
    )
}

/// Filesystem failure with the affected path and the original operating system cause.
#[derive(Debug)]
struct PathError {
    /// Source path whose operation failed.
    path: PathBuf,
    /// Original I/O error, available through the standard error-source interface.
    cause: io::Error,
}

impl std::fmt::Display for PathError {
    /// Formats the affected path alongside the underlying failure.
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}: {}", self.path.display(), self.cause)
    }
}

impl std::error::Error for PathError {
    /// Returns the original operating system failure for error-chain inspection.
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.cause)
    }
}
