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

The block-padding rule gives multiline blocks the blank lines they need. `if`,
`while`, `for`, `loop`, `match`, and other multiline block constructs are separated
from neighboring statements. Complex executable bodies also get blank lines
inside their braces, while tiny bodies, connected expressions, and comments stay
together.

## Install

Build with Rust 1.90 or newer. Install Clippy for the toolchain you use to lint:

```sh
rustup component add clippy
cargo install --git https://github.com/milewski/bclippy --locked
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
        record_progress();

    }

    finish();
}
```

`bclippy::block_padding` pads complete statements, items, and match arms. It also
handles scopes, closures, `unsafe`, `async`, `const`, and struct literals. A block
inside an expression is padded around its containing statement, keeping function
arguments and method chains compatible with rustfmt.

Inside executable blocks, a body needs padding when it has multiple statements,
a statement spanning multiple source lines, or nested brace syntax. This applies
recursively to loops, conditionals, async blocks, closures, and other executable
scopes. For example, the loop and async body below get padding while the tiny
`if` body stays compact:

```rust
async fn wait() {
    let response = tokio::time::timeout(Duration::from_secs(15), async {

        loop {

            if let Ok(response) = client.get(&address).send().await {
                break response;
            }

            tokio::time::sleep(Duration::from_millis(20)).await;

        }

    })
    .await;
}
```

Function and method bodies keep their outer edges compact. Empty bodies,
single-line blocks, struct fields, and declaration containers get no inner
padding.

Existing blank lines, literal contents, comments, attributes, and LF/CRLF endings
are preserved. `else` and `let … else` stay connected. Macro invocations are handled
as complete units; their opaque bodies are not rewritten.

Run `cargo fmt` first, then `cargo bclippy --fix`: rustfmt removes inner padding,
so rerun bclippy after formatting. With no paths, the source rule scans the
current directory recursively, excluding hidden directories, `target`,
`vendor`, `node_modules`, and symlinks. All selected files must parse before any
fix is written. Invalid syntax and filesystem failures return an error.

## CI

After installing a reviewed bclippy revision in your workflow:

```yaml
- name: Check Rust block padding
  run: cargo bclippy --check
```

Keep your Clippy checks alongside it. When checking formatting, account for the
formatter order: `cargo fmt --check` alone rejects the inner padding that bclippy
adds. Use `--fix` locally; the check command only reports file/line diagnostics.

## Develop

```sh
cargo fmt --all -- --check
cargo clippy --all-targets --all-features --locked -- -D warnings
cargo test --all-targets --all-features --locked
```

The rule implementation builds on stable Rust with `syn`. CI covers Linux,
macOS, Windows, and the minimum supported compiler.
