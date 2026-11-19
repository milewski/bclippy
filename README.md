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
inside their braces. A body containing just one expression stays compact, including
a multiline call; any existing inner padding around it is removed.
Standalone scoped blocks also keep their edges compact, regardless of body size.
Statements are also grouped visually: bindings stay together, and a blank line
separates different statement kinds or macro names.

## Install

Build with Rust 1.90 or newer. Install Clippy for the toolchain you use to lint:

```sh
rustup component add clippy
cargo install --git https://github.com/milewski/bclippy --locked
```

This installs **one executable**, `cargo-bclippy`, available as `cargo bclippy`.

## Check it. Fix it.

```sh
cargo bclippy --check                 # report incorrect padding; write nothing
cargo bclippy --fix                   # insert missing or remove redundant padding
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

Inside control-flow and value-producing blocks, a body needs padding when it has
multiple statements or a complex declaration. A single expression needs no inner
padding, even if it spans multiple source lines or contains nested braces. Expression statements with
a semicolon and standalone macro invocations follow the same rule. The fixer
removes existing empty lines at their body edges. A single multiline declaration
such as `let result = process(...);` retains the declaration-padding rule.

This applies recursively to loops, conditionals, async blocks, closures, and other
executable scopes. For example, the loop below gets padding because it has two
statements. The async body has only that loop expression and stays compact:

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

A closure containing one multiline call stays compact too:

```rust
fn configure() {
    let cases = [("Ollama cloud models not turned off (OLLAMA_NO_CLOUD)", |machine| {
        machine.succeeding.insert(
            "systemctl show ollama.service --property=LoadState --property=Environment".into(),
            "LoadState=loaded\nEnvironment=OLLAMA_HOST=127.0.0.1:11434\n".into(),
        );
    })];
}
```

Standalone scoped blocks, including `#[cfg(...)]` and labeled scopes, always keep
their contents next to the braces; existing blank lines at those edges are removed.
Their inner statements still get grouping and nested control-flow padding:

```rust
fn configure() {
    #[cfg(windows)]
    {
        directories.push(local_data.join("Programs/OpenAI/Codex/bin"));
        directories.push(local_data.join("Programs/nodejs"));

        if let Some(program_files) = program_files {
            directories.push(PathBuf::from(program_files).join("nodejs"));
        }
    }
}
```

Block-valued initializers, closure bodies, match arms, and `else` bodies retain
the executable-body padding rule. Function and method bodies keep their outer
edges compact. Empty bodies, single-line blocks, struct fields, and declaration
containers get no inner padding.

Existing blank lines outside single-expression and standalone-scope body-edge
gaps are preserved, as are literal contents, comments, attributes, and LF/CRLF endings. Blank lines inside
nested block comments or literals are never removed. Gaps containing opaque syntax
such as inner attributes are left unchanged. `else` and `let … else` stay connected.
Macro invocations are handled as complete units; their opaque bodies are not rewritten.

## Visual statement groups

Within an executable body, consecutive `let` bindings, local item declarations,
ordinary expressions, and standalone macro invocations form separate groups.
Each full macro path is its own group, so `assert_eq!` and `assert!` are separated:

```rust
fn check_icon() {
    let index = find_index();
    let icon = find_icon();

    assert_eq!(icon.content_type, "image/png");
    assert_eq!(icon.bytes, expected_bytes());

    assert!(index.has_icon());
    assert!(icon.is_local());
}
```

Bindings use the containing `let` statement's group, including macro initializers.
Ordinary calls share the expression group. Qualified macro paths retain their
identity; aliases are not resolved. Existing blank lines and multiline block
separation remain in effect, and leading comments and attributes stay attached
to their statements. Statement order, literal contents, and opaque macro bodies
are preserved. Function and method edges stay compact. Multi-statement control-flow
and value-producing blocks retain their inner padding. Standalone scopes and
single-expression bodies stay unpadded.

The same `--check` and `--fix` commands apply these groups automatically.

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
