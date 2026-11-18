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
    let output = wrapper_without_cargo().args(["bclippy", "--help"]).output().unwrap();

    assert!(output.status.success());
    assert!(String::from_utf8(output.stdout).unwrap().contains("Usage: cargo bclippy"));
    assert_eq!(output.stderr, b"");
}

#[test]
fn wrapper_version_succeeds_without_cargo() {
    let output = wrapper_without_cargo().arg("--version").output().unwrap();

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
    fn flags_after_separator_are_forwarded_without_interception() {
        let fixture = CargoFixture::new("printf '%s\\000' \"$@\"");

        for argument in ["--help", "-h", "--version", "-V", "--check", "--fix"] {
            let output = fixture.command().args(["--", argument]).output().unwrap();

            assert_eq!(output.stdout, format!("clippy\0--\0{argument}\0").as_bytes());
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

    #[test]
    fn padding_check_fails_without_modifying_source_or_starting_cargo() {
        let fixture = CargoFixture::new("exit 99");
        let source = "fn example() {\n    before();\n    if ready {\n        work();\n    }\n    after();\n}\n";
        let path = fixture.directory.join("example.rs");
        fs::write(&path, source).unwrap();
        let output = fixture.command().args(["--check", "example.rs"]).output().unwrap();

        assert_eq!(output.status.code(), Some(1));
        assert!(String::from_utf8(output.stderr).unwrap().contains("[bclippy::block_padding]"));
        assert_eq!(fs::read_to_string(path).unwrap(), source);
    }

    #[test]
    fn padding_fix_then_check_succeeds_without_cargo() {
        let fixture = CargoFixture::new("exit 99");
        let source = "fn example() {\n    before();\n    while ready {\n        work();\n    }\n    after();\n}\n";
        let path = fixture.directory.join("example.rs");
        fs::write(&path, source).unwrap();
        let output = fixture.command().args(["bclippy", "--fix", "example.rs"]).output().unwrap();

        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        assert_eq!(
            fs::read_to_string(path).unwrap(),
            source
                .replace("    while ready", "\n    while ready")
                .replace("    after();", "\n    after();")
        );
        assert!(fixture.command().args(["--check", "example.rs"]).status().unwrap().success());
    }

    #[test]
    fn padding_cli_reports_and_fixes_nested_async_body_padding() {
        let fixture = CargoFixture::new("exit 99");
        let source = include_str!("fixtures/timeout_unpadded.txt");
        let expected = include_str!("fixtures/timeout_padded.txt");
        let path = fixture.directory.join("example.rs");
        fs::write(&path, source).unwrap();
        let output = fixture.command().args(["--check", "example.rs"]).output().unwrap();

        assert_eq!(output.status.code(), Some(1));
        assert_eq!(fs::read_to_string(&path).unwrap(), source);
        assert!(String::from_utf8(output.stderr).unwrap().contains("inside a multiline block"));

        let output = fixture.command().args(["--fix", "example.rs"]).output().unwrap();

        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        assert_eq!(fs::read_to_string(&path).unwrap(), expected);
        assert!(fixture.command().args(["--check", "example.rs"]).status().unwrap().success());
        assert!(fixture.command().args(["--fix", "example.rs"]).status().unwrap().success());
        assert_eq!(fs::read_to_string(path).unwrap(), expected);
    }

    #[test]
    fn padding_cli_rejects_and_removes_single_expression_inner_padding() {
        let fixture = CargoFixture::new("exit 99");
        let source = include_str!("fixtures/ollama_padded.txt");
        let expected = include_str!("fixtures/ollama_compact.txt");
        let path = fixture.directory.join("example.rs");
        fs::write(&path, source).unwrap();

        let checked = fixture.command().args(["--check", "example.rs"]).output().unwrap();

        assert_eq!(checked.status.code(), Some(1));
        assert_eq!(fs::read_to_string(&path).unwrap(), source);
        let diagnostics = String::from_utf8(checked.stderr).unwrap();
        assert!(diagnostics.contains("[bclippy::block_padding]"));
        assert!(diagnostics.contains("example.rs:4:1:"));
        assert!(diagnostics.contains("example.rs:9:1:"));

        let fixed = fixture.command().args(["--fix", "example.rs"]).output().unwrap();

        assert!(fixed.status.success(), "{}", String::from_utf8_lossy(&fixed.stderr));
        assert_eq!(fs::read_to_string(&path).unwrap(), expected);
        assert!(fixture.command().args(["--check", "example.rs"]).status().unwrap().success());
        assert!(fixture.command().args(["--fix", "example.rs"]).status().unwrap().success());
        assert_eq!(fs::read_to_string(path).unwrap(), expected);
    }

    #[test]
    fn parse_failure_prevents_all_planned_writes() {
        let fixture = CargoFixture::new("exit 99");
        let source = "fn example() {\n    before();\n    match value {\n        _ => work(),\n    }\n}\n";
        let path = fixture.directory.join("a_valid.rs");
        fs::write(&path, source).unwrap();
        let padded_path = fixture.directory.join("b_padded.rs");
        let padded_source = include_str!("fixtures/ollama_padded.txt");
        fs::write(&padded_path, padded_source).unwrap();
        fs::write(fixture.directory.join("z_invalid.rs"), "fn incomplete( {").unwrap();
        let output = fixture.command().arg("--fix").output().unwrap();

        assert!(!output.status.success());
        assert!(String::from_utf8(output.stderr).unwrap().contains("z_invalid.rs"));
        assert_eq!(fs::read_to_string(path).unwrap(), source);
        assert_eq!(fs::read_to_string(padded_path).unwrap(), padded_source);
    }

    #[test]
    fn recursive_discovery_skips_build_vendor_hidden_and_symlink_sources() {
        let fixture = CargoFixture::new("exit 99");
        fs::write(fixture.directory.join("example.rs"), "fn example() {}\n").unwrap();

        for directory in ["target", "vendor", "node_modules", ".hidden"] {
            let path = fixture.directory.join(directory);
            fs::create_dir(&path).unwrap();
            fs::write(path.join("invalid.rs"), "fn incomplete( {").unwrap();
        }

        std::os::unix::fs::symlink("vendor/invalid.rs", fixture.directory.join("linked.rs")).unwrap();
        let output = fixture.command().arg("--check").output().unwrap();

        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        assert!(String::from_utf8(output.stderr).unwrap().contains("checked 1 Rust files"));
    }

    #[test]
    fn empty_source_selection_is_an_error() {
        let fixture = CargoFixture::new("exit 99");
        let output = fixture.command().arg("--check").output().unwrap();

        assert!(!output.status.success());
        assert!(String::from_utf8(output.stderr).unwrap().contains("no Rust source files"));
    }

    #[test]
    fn conflicting_operations_fail_without_writing_source() {
        let fixture = CargoFixture::new("exit 99");
        let source = "fn example() {\n    before();\n    if ready {\n        work();\n    }\n}\n";
        let path = fixture.directory.join("example.rs");
        fs::write(&path, source).unwrap();

        for arguments in [["--check", "--fix", "example.rs"], ["--fix", "--check", "example.rs"]] {
            let output = fixture.command().args(arguments).output().unwrap();

            assert!(!output.status.success());
            assert!(String::from_utf8(output.stderr).unwrap().contains("choose either --check or --fix"));
            assert_eq!(fs::read_to_string(&path).unwrap(), source);
        }
    }
}
