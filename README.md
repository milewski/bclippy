# bclippy

Extend your Rust lint workflow while keeping upstream Cargo and Clippy.

`bclippy` is a Cargo subcommand that runs upstream Clippy and provides additional
source-level rules. Ordinary invocations delegate to the Clippy installed for
your selected Rust toolchain, forwarding its arguments, diagnostics, and exit
status. Dedicated commands check and fix multiline block padding without
compiling the project or maintaining a fork of Cargo or Clippy.

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
The source rule uses `syn` to parse Rust on stable toolchains. It does not link
against Rust compiler internals.

## Usage

Run these commands in the Rust project you want to check:

```sh
cargo bclippy
cargo bclippy --workspace --all-targets --all-features -- -D warnings
cargo bclippy --message-format=json
```

Cargo options go before `--`; compiler and lint options go after it, just as
with `cargo clippy`. This package installs one executable, `cargo-bclippy`,
which provides both formatting operations through flags:

```sh
cargo bclippy --help
cargo bclippy --version
```

`--help` describes the check/fix options; `--version` displays bclippy's version.
`--check` and `--fix` are mutually exclusive. Other invocations delegate to
upstream Clippy. Run `cargo clippy --fix` for Clippy's own compiler suggestions.

Rustup toolchain selection works through Cargo:

```sh
rustup component add clippy --toolchain stable
cargo +stable bclippy --all-targets
```

## Multiline block padding

The `bclippy::block_padding` rule requires a blank line between a multiline brace
construct and a neighboring statement or item. This includes `if`/`else`,
`while`, `for`, `loop`, `match`, plain and labeled scopes, `unsafe`, `async`,
`const`, closures, function bodies, and struct literals. A block expression in
an assignment or function argument is padded around the complete containing
statement. Match arms with multiline blocks are separated as complete arms.
Function arguments, struct fields, and method chains are not split; this keeps
the rule compatible with stable rustfmt.

```rust
fn read() {
    before();

    if ready() {
        work();
    }

    after();
}
```

Check without writing, or apply fixes:

```sh
cargo bclippy --check
cargo bclippy --fix
cargo bclippy --fix crates/example/src/main.rs
cargo bclippy --check src tests
```

These modes run only the source rule; they do not start Clippy. Use them as a
formatting step alongside the ordinary `cargo bclippy` invocation. Exit status
is zero for compliant sources or successful fixes, and nonzero for missing
padding in check mode, invalid Rust, missing sources, or filesystem errors.
Diagnostics identify the file, original line, and rule.

Paths select files or directories recursively and default to the current
directory. Hidden directories, `target`, `vendor`, `node_modules`, and symlinks
are skipped. Overlapping selections are deduplicated. Explicit paths do not
select Cargo packages or features; they select source files, including code
behind `cfg`. Use `--` before a dash-prefixed path.

Run `cargo fmt` first to place constructs on their own lines, then run the
bclippy fixer. Fixes only insert empty lines at existing line boundaries;
they preserve comments, literal contents, indentation, and LF/CRLF endings.
Existing blank lines are retained. No padding is added immediately inside
opening/closing delimiters. `else` chains, `let … else`, attributes, and
leading comments stay attached. Macro invocations can be padded as units;
their opaque bodies are not rewritten or expanded.

Every selected source is parsed before writing any fix, so invalid syntax
aborts the operation without changing files. Write failures can still leave
earlier files fixed. Sources edited after planning are refused instead of
being overwritten. Fixing an already compliant file makes no changes.

## Upstream updates

bclippy invokes the Cargo executable provided by Cargo's `CARGO` environment
variable, falling back to `cargo` on `PATH` when that variable is absent. It runs
the `clippy` subcommand and leaves its output unchanged, including JSON output.

Clippy stays managed by your Rust toolchain. Updating that toolchain updates
Clippy without rebuilding bclippy. A project that pins its toolchain keeps using
that pinned version.

Compiler-aware custom lints could use a separate Dylint stage, with its own
compiler compatibility requirements; that integration is not implemented. See the
[Cargo extension documentation](https://doc.rust-lang.org/cargo/reference/external-tools.html)
and [Dylint](https://github.com/trailofbits/dylint).

## Development

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --all-targets --all-features --locked
cargo test --workspace --all-features --doc --locked
cargo doc --workspace --all-features --no-deps --document-private-items --locked
```

Exercise the wrapper against this repository without installing it:

```sh
cargo build --locked
cargo run --locked -- --bclippy-help
cargo run --locked -- --check src tests
cargo run --locked -- --workspace --all-targets --all-features --locked -- -D warnings
```

CI runs formatting, strict linting, tests, documentation, and a Cargo subcommand
smoke check on Linux, macOS, and Windows. Rust 1.90 is checked separately as the
minimum supported compiler.
