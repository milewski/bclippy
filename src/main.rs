//! Cargo subcommand that delegates to the selected toolchain's upstream Clippy.

use std::env;
use std::ffi::OsString;
use std::io::{self, Write};
use std::path::Path;
use std::process::{self, Command};

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

/// Handles wrapper information flags or delegates all arguments to upstream Clippy.
///
/// Cargo adds the subcommand name as the first argument; direct invocation omits it.
/// Arguments remain operating-system strings so paths need not be valid UTF-8.
///
/// # Errors
/// Returns an error if wrapper output cannot be written or Cargo cannot be executed.
fn run() -> io::Result<i32> {
    let mut arguments = env::args_os().skip(1).peekable();
    if arguments.peek().is_some_and(|argument| argument == "bclippy") {
        arguments.next();
    }
    let arguments: Vec<OsString> = arguments.collect();

    match arguments.as_slice() {
        [argument] if argument == "--bclippy-help" => {
            writeln!(
                io::stdout().lock(),
                "bclippy - extend upstream Clippy\n\n\
                 Usage: cargo bclippy [CARGO OPTIONS] [-- CLIPPY OPTIONS]\n\n\
                 All arguments are forwarded unchanged to cargo clippy.\n\
                 Use --help or --version for upstream Clippy information.\n\n\
                 Wrapper options (used alone):\n\
                   --bclippy-help       Show this help\n\
                   --bclippy-version    Show the bclippy version"
            )?;
            Ok(0)
        }
        [argument] if argument == "--bclippy-version" => {
            writeln!(io::stdout().lock(), "bclippy {}", env!("CARGO_PKG_VERSION"))?;
            Ok(0)
        }
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
