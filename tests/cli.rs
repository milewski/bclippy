//! Process-boundary checks for Cargo dispatch and transparent Clippy delegation.

use std::process::Command;

/// Starts the built wrapper with Cargo deliberately pointing to a nonexistent path.
fn wrapper_without_cargo() -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_cargo-bclippy"));
    command.env("CARGO", "/bclippy-test-missing-cargo");
    command
}

#[test]
fn wrapper_help_succeeds_without_cargo() {
    let output = wrapper_without_cargo().args(["bclippy", "--bclippy-help"]).output().unwrap();

    assert!(output.status.success());
    assert!(String::from_utf8(output.stdout).unwrap().contains("Usage: cargo bclippy"));
    assert_eq!(output.stderr, b"");
}

#[test]
fn wrapper_version_succeeds_without_cargo() {
    let output = wrapper_without_cargo().arg("--bclippy-version").output().unwrap();

    assert!(output.status.success());
    assert_eq!(output.stdout, format!("bclippy {}\n", env!("CARGO_PKG_VERSION")).as_bytes());
}

#[test]
fn missing_cargo_reports_an_actionable_failure() {
    let output = wrapper_without_cargo().output().unwrap();

    assert!(!output.status.success());
    assert_eq!(output.stdout, b"");
    assert!(String::from_utf8(output.stderr).unwrap().contains("Ensure Rust is installed"));
}

#[cfg(unix)]
mod unix {
    use std::ffi::OsString;
    use std::fs;
    use std::io::Write;
    use std::os::unix::ffi::OsStringExt;
    use std::os::unix::fs::PermissionsExt;
    use std::os::unix::process::ExitStatusExt;
    use std::path::PathBuf;
    use std::process::{Command, Stdio};
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// Allocates distinct directories even when test cases run concurrently.
    static NEXT_DIRECTORY: AtomicUsize = AtomicUsize::new(0);

    /// Owns an isolated executable standing in for Cargo and its working directory.
    struct CargoFixture {
        /// Directory removed after the test finishes.
        directory: PathBuf,
        /// Executable selected through `CARGO` or `PATH`.
        executable: PathBuf,
    }

    impl CargoFixture {
        /// Creates a fake Cargo executable with the supplied shell-script body.
        fn new(script: &str) -> Self {
            let directory = std::env::temp_dir().join(format!(
                "bclippy-test-{}-{}",
                std::process::id(),
                NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&directory).unwrap();
            let directory = fs::canonicalize(directory).unwrap();
            let executable = directory.join("cargo");
            fs::write(&executable, format!("#!/bin/sh\n{script}\n")).unwrap();
            fs::set_permissions(&executable, fs::Permissions::from_mode(0o755)).unwrap();
            Self { directory, executable }
        }

        /// Creates a wrapper command that delegates to this fixture through `CARGO`.
        fn command(&self) -> Command {
            let mut command = Command::new(env!("CARGO_BIN_EXE_cargo-bclippy"));
            command.env("CARGO", &self.executable).current_dir(&self.directory);
            command
        }
    }

    impl Drop for CargoFixture {
        /// Removes temporary files without hiding an existing test failure.
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.directory);
        }
    }

    #[test]
    fn cargo_dispatch_forwards_arguments_and_separator_without_rewriting() {
        let fixture = CargoFixture::new("printf '%s\\000' \"$@\"");
        let output = fixture
            .command()
            .args([
                "bclippy",
                "--workspace",
                "--manifest-path",
                "path with spaces/Cargo.toml",
                "--",
                "-D",
                "warnings",
            ])
            .output()
            .unwrap();

        assert!(output.status.success());
        assert_eq!(
            output.stdout,
            b"clippy\0--workspace\0--manifest-path\0path with spaces/Cargo.toml\0--\0-D\0warnings\0"
        );
    }

    #[test]
    fn direct_invocation_keeps_its_first_argument() {
        let fixture = CargoFixture::new("printf '%s\\000' \"$@\"");
        let output = fixture
            .command()
            .args(["--all-targets", "--", "-Wclippy::pedantic"])
            .output()
            .unwrap();

        assert_eq!(output.stdout, b"clippy\0--all-targets\0--\0-Wclippy::pedantic\0");
    }

    #[test]
    fn dispatch_removes_only_one_subcommand_argument() {
        let fixture = CargoFixture::new("printf '%s\\000' \"$@\"");
        let output = fixture.command().args(["bclippy", "bclippy"]).output().unwrap();

        assert_eq!(output.stdout, b"clippy\0bclippy\0");
    }

    #[test]
    fn upstream_help_and_version_flags_are_forwarded() {
        let fixture = CargoFixture::new("printf '%s\\000' \"$@\"");

        for argument in ["--help", "-h", "--version", "-V"] {
            let output = fixture.command().arg(argument).output().unwrap();

            assert_eq!(output.stdout, format!("clippy\0{argument}\0").as_bytes());
        }
    }

    #[test]
    fn wrapper_flags_after_separator_are_forwarded() {
        let fixture = CargoFixture::new("printf '%s\\000' \"$@\"");
        let output = fixture.command().args(["--", "--bclippy-help"]).output().unwrap();

        assert_eq!(output.stdout, b"clippy\0--\0--bclippy-help\0");
    }

    #[test]
    fn non_utf8_arguments_reach_cargo_unchanged() {
        let fixture = CargoFixture::new("printf '%s\\000' \"$@\"");
        let output = fixture.command().arg(OsString::from_vec(b"path-\xff".to_vec())).output().unwrap();

        assert_eq!(output.stdout, b"clippy\0path-\xff\0");
    }

    #[test]
    fn upstream_output_and_nonzero_exit_status_are_preserved() {
        let fixture = CargoFixture::new("printf 'upstream stdout\\n'; printf 'upstream stderr\\n' >&2; exit 37");
        let output = fixture.command().output().unwrap();

        assert_eq!(output.status.code(), Some(37));
        assert_eq!(output.stdout, b"upstream stdout\n");
        assert_eq!(output.stderr, b"upstream stderr\n");
    }

    #[test]
    fn cargo_environment_variable_takes_precedence_over_path() {
        let fixture = CargoFixture::new("printf 'selected through CARGO\\n'");
        let other_fixture = CargoFixture::new("printf 'selected through PATH\\n'");
        let output = fixture.command().env("PATH", &other_fixture.directory).output().unwrap();

        assert_eq!(output.stdout, b"selected through CARGO\n");
    }

    #[test]
    fn cargo_is_resolved_from_path_when_environment_override_is_absent() {
        let fixture = CargoFixture::new("printf 'selected through PATH\\n'");
        let output = fixture
            .command()
            .env_remove("CARGO")
            .env("PATH", &fixture.directory)
            .output()
            .unwrap();

        assert_eq!(output.stdout, b"selected through PATH\n");
    }

    #[test]
    fn cargo_inherits_working_directory_environment_and_standard_input() {
        let fixture = CargoFixture::new("read -r input; printf '%s\\n%s\\n%s\\n' \"$PWD\" \"$BCLIPPY_TEST_VALUE\" \"$input\"");
        let mut child = fixture
            .command()
            .env("BCLIPPY_TEST_VALUE", "inherited environment")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        child.stdin.take().unwrap().write_all(b"inherited stdin\n").unwrap();
        let output = child.wait_with_output().unwrap();

        assert_eq!(
            output.stdout,
            format!("{}\ninherited environment\ninherited stdin\n", fixture.directory.display()).as_bytes()
        );
    }

    #[test]
    fn upstream_signal_termination_is_preserved() {
        let fixture = CargoFixture::new("kill -TERM $$");
        let output = fixture.command().output().unwrap();

        assert_eq!(output.status.signal(), Some(15));
    }
}
