<p align="center">
  <img src="assets/logo.png" alt="bclippy crab mascot with curly-brace claws and a paperclip" width="220" />
</p>

<h1 align="center">bclippy</h1>

<p align="center"><strong>A little breathing room for your Rust code.</strong></p>

<p align="center">
  <a href="https://github.com/milewski/bclippy/actions/workflows/ci.yml"><img src="https://github.com/milewski/bclippy/actions/workflows/ci.yml/badge.svg?branch=main" alt="CI" /></a>
  &nbsp; Stable Rust &nbsp;·&nbsp; One executable &nbsp;·&nbsp; Upstream Clippy
</p>

bclippy extends your Cargo and Clippy workflow with focused, automatically fixable
source rules. It keeps using the Clippy installed for your selected Rust toolchain,
so upstream improvements keep arriving with your Rust updates.

The first rule gives multiline blocks the blank lines they need. `if`, `while`,
`for`, `loop`, `match`, and other multiline block constructs are separated from
neighboring statements while connected expressions and comments stay together.

> **Development status:** the check/fix rule is being developed in
> [PR #1](https://github.com/milewski/bclippy/pull/1). The installation below selects
> that branch. The current `main` implementation provides the original Clippy wrapper.

## Install

Build with Rust 1.90 or newer. Install Clippy for the toolchain you use to lint:

```sh
rustup component add clippy
cargo install --git https://github.com/milewski/bclippy \
  --branch feat/multiline-block-padding --locked
```

This installs **one executable**, `cargo-bclippy`, available as `cargo bclippy`.

## Check it. Fix it.

```sh
cargo bclippy --check                 # report missing padding; write nothing
cargo bclippy --fix                   # insert missing blank lines
cargo bclippy --check src tests       # select files or directories
cargo bclippy --help
```

`--check` exits with status **1** when any selected file violates the rule, so CI
fails. `--fix` applies the same rule locally. These flags are mutually exclusive.
Neither mode needs to compile your project.

Ordinary invocations still run upstream Clippy:

```sh
cargo bclippy --workspace --all-targets -- -D warnings
```

## What the first rule does

```rust
fn process() {
    prepare();

    while has_work() {
        process_next();
    }

    finish();
}
```

`bclippy::block_padding` pads complete statements, items, and match arms. It also
handles scopes, closures, `unsafe`, `async`, `const`, and struct literals. A block
inside an expression is padded around its containing statement, keeping function
arguments and method chains compatible with rustfmt.

Existing blank lines, literal contents, comments, attributes, and LF/CRLF endings
are preserved. `else` and `let … else` stay connected. Macro invocations are handled
as complete units; their opaque bodies are not rewritten.

Run `cargo fmt` first, then `cargo bclippy --fix`. With no paths, the source rule
scans the current directory recursively, excluding hidden directories, `target`,
`vendor`, `node_modules`, and symlinks. All selected files must parse before any
fix is written. Invalid syntax and filesystem failures return an error.

## CI

After installing a reviewed bclippy revision in your workflow:

```yaml
- name: Check Rust block padding
  run: cargo bclippy --check
```

Keep your existing rustfmt and Clippy checks alongside it. Use `--fix` locally;
CI only checks and reports file/line diagnostics.

## Develop

```sh
cargo fmt --all -- --check
cargo clippy --all-targets --all-features --locked -- -D warnings
cargo test --all-targets --all-features --locked
```

The rule implementation builds on stable Rust with `syn`. CI covers Linux,
macOS, Windows, and the minimum supported compiler.
