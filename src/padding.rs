//! Blank-line padding around multiline Rust constructs at syntax boundaries.

use std::collections::BTreeSet;
use std::ops::Range;

use proc_macro2::{Delimiter, TokenStream, TokenTree};
use quote::ToTokens;
use syn::spanned::Spanned;
use syn::visit::{self, Visit};

/// Parsed Rust source whose original bytes remain available for whitespace-only fixes.
pub struct RustSource<'source> {
    /// Original source, including comments, literal contents, and line endings.
    source: &'source str,
    /// Syntax that identifies safe boundaries between complete constructs.
    syntax: syn::File,
    /// Bytes stripped by syn when parsing an optional BOM or executable shebang.
    syntax_offset: usize,
}

impl<'source> RustSource<'source> {
    /// Parses a complete Rust file without compiling it or expanding its macros.
    ///
    /// # Errors
    /// Returns the syntax error for invalid or unsupported Rust; no source is changed.
    ///
    /// # Examples
    /// ```
    /// use bclippy::padding::RustSource;
    ///
    /// let source = RustSource::parse("fn main() {}")?;
    /// assert_eq!(source.padded().source(), "fn main() {}");
    /// # Ok::<(), syn::Error>(())
    /// ```
    pub fn parse(source: &'source str) -> Result<Self, syn::Error> {
        let syntax = syn::parse_file(source)?;
        let syntax_offset = source.len() - source.trim_start_matches('\u{feff}').len() + syntax.shebang.as_deref().map_or(0, str::len);

        Ok(Self {
            source,
            syntax,
            syntax_offset,
        })
    }

    /// Inserts missing empty lines around multiline constructs and inside complex blocks.
    ///
    /// Sibling statements, items, and match arms are separated at safe boundaries.
    /// Executable blocks receive inner padding when their bodies contain multiple
    /// statements, a multiline statement, or nested braces. Tiny one-line bodies
    /// and function-body edges stay compact. Connected syntax and macro bodies
    /// stay intact; comments, existing blank lines, and LF/CRLF endings are preserved.
    #[must_use]
    pub fn padded(&self) -> Padding {
        let mut visitor = PaddingVisitor {
            source: self.source,
            insertions: BTreeSet::new(),
            syntax_offset: self.syntax_offset,
        };

        visitor.visit_file(&self.syntax);

        let mut output = String::with_capacity(self.source.len() + visitor.insertions.len() * 2);
        let mut previous_offset = 0;
        let mut original_line = 1;
        let mut lines = Vec::with_capacity(visitor.insertions.len());

        for offset in visitor.insertions {
            let segment = &self.source[previous_offset..offset];
            original_line += segment.bytes().filter(|byte| *byte == b'\n').count();
            output.push_str(segment);
            output.push_str(if self.source[..offset].ends_with("\r\n") { "\r\n" } else { "\n" });
            lines.push(original_line);
            previous_offset = offset;
        }

        output.push_str(&self.source[previous_offset..]);
        Padding { output, lines }
    }
}

/// A whitespace-only replacement and the original line numbers that need padding.
pub struct Padding {
    /// Complete source with missing empty lines inserted.
    output: String,
    /// Original one-based lines immediately following an inserted empty line.
    lines: Vec<usize>,
}

impl Padding {
    /// Returns the complete fixed source without changing any non-padding bytes.
    #[must_use]
    pub fn source(&self) -> &str {
        &self.output
    }

    /// Returns one-based original line numbers for diagnostics, in ascending order.
    #[must_use]
    pub fn lines(&self) -> &[usize] {
        &self.lines
    }
}

/// A complete sibling construct and whether it contains a multiline brace group.
struct Construct {
    /// Original byte range of the entire statement, item, or match arm.
    range: Range<usize>,
    /// Whether this construct requires separation from neighboring constructs.
    multiline: bool,
}

/// Collects deduplicated insertion positions without rewriting Rust tokens.
struct PaddingVisitor<'source> {
    /// Original file used to distinguish empty lines from lines inside comments.
    source: &'source str,
    /// Safe line starts needing exactly one extra line ending.
    insertions: BTreeSet<usize>,
    /// Original prefix length omitted from the syntax parser's byte coordinates.
    syntax_offset: usize,
}

impl PaddingVisitor<'_> {
    /// Pads a complex executable body's edges without touching function-body edges.
    fn block_body(&mut self, block: &syn::Block) {
        let (Some(first), Some(last)) = (block.stmts.first(), block.stmts.last()) else {
            return;
        };

        if block.brace_token.span.open().start().line == block.brace_token.span.close().end().line
            || (block.stmts.len() == 1 && first.span().start().line == first.span().end().line && !has_braces(first.to_token_stream()))
        {
            return;
        }

        let opening = block.brace_token.span.open().byte_range().end + self.syntax_offset;
        let first_start = first.span().byte_range().start + self.syntax_offset;

        if let Some(offset) = missing_gap(self.source, opening..first_start) {
            self.insertions.insert(offset);
        }

        let last_end = last.span().byte_range().end + self.syntax_offset;
        let closing = block.brace_token.span.close().byte_range().start + self.syntax_offset;

        if missing_gap(self.source, last_end..closing).is_some() {
            let closing_line = self.source[..closing].rfind('\n').map_or(0, |offset| offset + 1);

            if closing_line >= last_end && self.source[closing_line..closing].bytes().all(|byte| byte.is_ascii_whitespace()) {
                self.insertions.insert(closing_line);
            }
        }
    }

    /// Checks adjacent complete constructs, never the edges of their container.
    fn sequence<'syntax, Syntax: Spanned + ToTokens + 'syntax>(&mut self, nodes: impl IntoIterator<Item = &'syntax Syntax>) {
        let constructs: Vec<_> = nodes
            .into_iter()
            .map(|node| Construct {
                range: {
                    let range = node.span().byte_range();
                    range.start + self.syntax_offset..range.end + self.syntax_offset
                },

                multiline: has_multiline_braces(node.to_token_stream()),
            })
            .collect();

        for neighbors in constructs.windows(2) {
            let previous = &neighbors[0];
            let next = &neighbors[1];

            if (previous.multiline || next.multiline)
                && let Some(offset) = missing_gap(self.source, previous.range.end..next.range.start)
            {
                self.insertions.insert(offset);
            }
        }
    }
}

impl<'syntax> Visit<'syntax> for PaddingVisitor<'_> {
    /// Pads executable expression bodies while item functions retain compact edges.
    fn visit_expr(&mut self, node: &'syntax syn::Expr) {
        match node {
            syn::Expr::Async(expression) => self.block_body(&expression.block),
            syn::Expr::Block(expression) => self.block_body(&expression.block),
            syn::Expr::Const(expression) => self.block_body(&expression.block),
            syn::Expr::ForLoop(expression) => self.block_body(&expression.body),
            syn::Expr::If(expression) => self.block_body(&expression.then_branch),
            syn::Expr::Loop(expression) => self.block_body(&expression.body),
            syn::Expr::TryBlock(expression) => self.block_body(&expression.block),
            syn::Expr::Unsafe(expression) => self.block_body(&expression.block),
            syn::Expr::While(expression) => self.block_body(&expression.body),
            _ => {}
        }

        visit::visit_expr(self, node);
    }

    /// Checks top-level items before recursively examining their bodies.
    fn visit_file(&mut self, node: &'syntax syn::File) {
        self.sequence(&node.items);
        visit::visit_file(self, node);
    }

    /// Checks statements, including a block's final expression.
    fn visit_block(&mut self, node: &'syntax syn::Block) {
        self.sequence(&node.stmts);
        visit::visit_block(self, node);
    }

    /// Checks items in inline modules without adding padding at the module edges.
    fn visit_item_mod(&mut self, node: &'syntax syn::ItemMod) {
        if let Some((_, items)) = &node.content {
            self.sequence(items);
        }

        visit::visit_item_mod(self, node);
    }

    /// Checks associated items inside implementations.
    fn visit_item_impl(&mut self, node: &'syntax syn::ItemImpl) {
        self.sequence(&node.items);
        visit::visit_item_impl(self, node);
    }

    /// Checks associated items inside traits, including default method bodies.
    fn visit_item_trait(&mut self, node: &'syntax syn::ItemTrait) {
        self.sequence(&node.items);
        visit::visit_item_trait(self, node);
    }

    /// Checks declarations inside foreign modules.
    fn visit_item_foreign_mod(&mut self, node: &'syntax syn::ItemForeignMod) {
        self.sequence(&node.items);
        visit::visit_item_foreign_mod(self, node);
    }

    /// Checks complete match arms so their patterns and bodies remain connected.
    fn visit_expr_match(&mut self, node: &'syntax syn::ExprMatch) {
        self.sequence(&node.arms);
        visit::visit_expr_match(self, node);
    }
}

/// Detects nested brace syntax, including inline blocks inside opaque macro tokens.
fn has_braces(tokens: TokenStream) -> bool {
    tokens.into_iter().any(|token| match token {
        TokenTree::Group(group) => group.delimiter() == Delimiter::Brace || has_braces(group.stream()),
        _ => false,
    })
}

/// Finds multiline brace groups using tokens, so braces in literals are ignored.
fn has_multiline_braces(tokens: TokenStream) -> bool {
    tokens.into_iter().any(|token| match token {
        TokenTree::Group(group) => {
            (group.delimiter() == Delimiter::Brace && group.span_open().start().line < group.span_close().end().line)
                || has_multiline_braces(group.stream())
        }

        _ => false,
    })
}

/// Locates an unpadded gap, keeping leading comments attached to the next construct.
///
/// Gaps contain only separators, whitespace, and comments. Nested block comments
/// are tracked so neither their contents nor their internal empty lines are edited.
fn missing_gap(source: &str, range: Range<usize>) -> Option<usize> {
    let gap = source.get(range.clone())?.as_bytes();
    let mut offset = 0;
    let mut comment_depth = 0;
    let mut line_has_content = true;
    let mut first_safe_line = None;

    while offset < gap.len() {
        let byte = gap[offset];
        let following = gap.get(offset + 1).copied();

        if byte == b'/' && following == Some(b'*') {
            comment_depth += 1;
            line_has_content = true;
            offset += 2;
        } else if comment_depth > 0 && byte == b'*' && following == Some(b'/') {
            comment_depth -= 1;
            offset += 2;
        } else if comment_depth == 0 && byte == b'/' && following == Some(b'/') {
            line_has_content = true;

            while offset < gap.len() && gap[offset] != b'\n' {
                offset += 1;
            }
        } else if byte == b'\n' {
            if comment_depth == 0 {
                if !line_has_content {
                    return None;
                }

                first_safe_line.get_or_insert(range.start + offset + 1);
                line_has_content = false;
            } else {
                line_has_content = true;
            }

            offset += 1;
        } else {
            line_has_content |= !byte.is_ascii_whitespace();
            offset += 1;
        }
    }

    first_safe_line
}
