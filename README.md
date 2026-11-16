# bclippy

Extend your Rust lint workflow while keeping upstream Cargo and Clippy.

`bclippy` is a Cargo subcommand. Its initial implementation delegates to the
Clippy installed for your selected Rust toolchain. It forwards arguments,
diagnostics, and exit status, providing a foundation for additional checks
without maintaining a fork of Cargo or Clippy.

Custom lint rules and Dylint integration are not implemented yet.

## Install

Build requirements: Rust 1.90 or newer. Clippy must be installed for the
toolchain you use to lint your project.

```sh
rustup component add clippy
cargo install --git https://github.com/milewski/bclippy --locked
```

For a local checkout:

```sh
cargo install --path . --locked
```

The installed executable is `cargo-bclippy`, available through `cargo bclippy`.
This project has no external dependencies.

## Usage

Run these commands in the Rust project you want to check:

```sh
cargo bclippy
cargo bclippy --workspace --all-targets --all-features -- -D warnings
cargo bclippy --message-format=json
```

Cargo options go before `--`; compiler and lint options go after it, just as
with `cargo clippy`. Standard help and version flags are forwarded to Clippy:

```sh
cargo bclippy --help
cargo bclippy --version
```

To display bclippy's own help or version, use these flags on their own:

```sh
cargo bclippy --bclippy-help
cargo bclippy --bclippy-version
```

Rustup toolchain selection works through Cargo:

```sh
rustup component add clippy --toolchain stable
cargo +stable bclippy --all-targets
```

## Upstream updates

bclippy invokes the Cargo executable provided by Cargo's `CARGO` environment
variable, falling back to `cargo` on `PATH` when that variable is absent. It runs
the `clippy` subcommand and leaves its output unchanged, including JSON output.

Clippy stays managed by your Rust toolchain. Updating that toolchain updates
Clippy without rebuilding bclippy. A project that pins its toolchain keeps using
that pinned version.

Future checks can be coordinated alongside this invocation. Compiler-aware
custom lints could use a separate Dylint stage, with its own compiler
compatibility requirements. See the
[Cargo extension documentation](https://doc.rust-lang.org/cargo/reference/external-tools.html)
and [Dylint](https://github.com/trailofbits/dylint).

## Development

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --all-targets --all-features --locked
cargo doc --workspace --all-features --no-deps --document-private-items --locked
```

Exercise the wrapper against this repository without installing it:

```sh
cargo build --locked
cargo run --locked -- --bclippy-help
cargo run --locked -- --workspace --all-targets --all-features --locked -- -D warnings
```

CI runs formatting, strict linting, tests, documentation, and a Cargo subcommand
smoke check on Linux, macOS, and Windows. Rust 1.90 is checked separately as the
minimum supported compiler.
