//! Cargo subcommand combining upstream Clippy with compiler-independent style rules.

mod formatting;

use std::env;
use std::ffi::OsString;
use std::io::{self, Write};
use std::path::Path;
use std::process::{self, Command};

use formatting::{FormatCommand, FormatMode};

/// Runs the wrapper and reports process-launch or output failures to standard error.
fn main() {
    match run() {
        Ok(exit_code) => process::exit(exit_code),

        Err(error) => {
            eprintln!("bclippy: {error}");
            process::exit(1);
        }
    }
}

/// Handles bclippy commands or delegates all arguments to upstream Clippy.
///
/// Cargo adds the subcommand name as the first argument; direct invocation omits it.
/// Arguments remain operating-system strings so paths need not be valid UTF-8.
///
/// # Errors
/// Returns an error if output cannot be written, a source operation fails, or Cargo cannot be executed.
fn run() -> io::Result<i32> {
    let mut arguments = env::args_os().skip(1).peekable();

    if arguments.peek().is_some_and(|argument| argument == "bclippy") {
        arguments.next();
    }

    let arguments: Vec<OsString> = arguments.collect();

    match arguments.as_slice() {
        [argument] if argument == "--help" || argument == "-h" || argument == "--bclippy-help" => {
            writeln!(
                io::stdout().lock(),
                "bclippy - extend upstream Clippy\n\n\
                 Usage: cargo bclippy --check [PATH ...]\n\
                        cargo bclippy --fix [PATH ...]\n\
                        cargo bclippy [CARGO OPTIONS] [-- CLIPPY OPTIONS]\n\n\
                 Options:\n\
                   --check       Report block padding errors without modifying files; exit 1 on violations\n\
                   --fix         Insert missing blank lines\n\
                   -h, --help    Show this help\n\
                   -V, --version Show the bclippy version\n\n\
                 One executable provides both source operations without compilation.\n\
                 Paths default to the current directory; use -- before dash-prefixed paths.\n\
                 Other invocations forward arguments to the selected upstream cargo clippy."
            )?;
            Ok(0)
        }

        [argument] if argument == "--version" || argument == "-V" || argument == "--bclippy-version" => {
            writeln!(io::stdout().lock(), "bclippy {}", env!("CARGO_PKG_VERSION"))?;
            Ok(0)
        }

        [argument, paths @ ..] if argument == "--check" => FormatCommand::new(FormatMode::Check, paths)?.run(),
        [argument, paths @ ..] if argument == "--fix" => FormatCommand::new(FormatMode::Fix, paths)?.run(),
        _ => run_clippy(&arguments),
    }
}

/// Executes the selected Cargo with `clippy` and the unmodified user arguments.
///
/// The `CARGO` environment variable takes precedence over looking up `cargo` on
/// `PATH`. Standard input, output, error, working directory, and environment are
/// inherited. On Unix the process is replaced to preserve signals and exit status.
///
/// # Errors
/// Returns an error when Cargo cannot be executed or its exit status is unavailable.
fn run_clippy(arguments: &[OsString]) -> io::Result<i32> {
    let cargo_program = env::var_os("CARGO").unwrap_or_else(|| OsString::from("cargo"));
    let mut command = Command::new(&cargo_program);
    command.arg("clippy").args(arguments);

    execute(&mut command).map_err(|error| {
        io::Error::new(
            error.kind(),
            format!(
                "could not execute Cargo '{}': {error}. \
                 Ensure Rust is installed and CARGO or PATH points to a working Cargo executable.",
                Path::new(&cargo_program).display()
            ),
        )
    })
}

/// Replaces this process with Cargo so its exit code and signals reach the caller.
///
/// # Errors
/// Returns the operating system's error if replacing the process fails.
#[cfg(unix)]
fn execute(command: &mut Command) -> io::Result<i32> {
    use std::os::unix::process::CommandExt;

    Err(command.exec())
}

/// Waits for Cargo and returns its numeric exit code on platforms without `exec`.
///
/// # Errors
/// Returns an error if Cargo cannot be started or waited for, or has no exit code.
#[cfg(not(unix))]
fn execute(command: &mut Command) -> io::Result<i32> {
    command
        .status()?
        .code()
        .ok_or_else(|| io::Error::other("Cargo terminated without a numeric exit code"))
}
