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

    /// Normalizes empty lines around multiline constructs and inside executable blocks.
    ///
    /// Sibling statements, items, and match arms are separated at safe boundaries.
    /// Changes between binding, item, expression, and distinct macro-path groups
    /// receive a blank line inside executable blocks.
    /// Executable blocks receive inner padding when their bodies contain multiple
    /// statements, a multiline declaration, or nested braces in a declaration.
    /// A lone expression or macro invocation stays compact even when multiline;
    /// existing inner padding is removed. Function-body edges get no padding.
    /// Standalone scoped block statements also keep compact edges regardless of
    /// body complexity, while their contents retain normal padding rules.
    /// Connected syntax, comments, literals, attributes, macro contents, and LF/CRLF
    /// endings stay intact. Existing padding elsewhere is preserved.
    #[must_use]
    pub fn padded(&self) -> Padding {
        let mut visitor = PaddingVisitor {
            source: self.source,
            insertions: BTreeSet::new(),
            removals: BTreeSet::new(),
            syntax_offset: self.syntax_offset,
        };

        visitor.visit_file(&self.syntax);

        let mut output = String::with_capacity(self.source.len() + visitor.insertions.len() * 2);
        let mut edits: Vec<_> = visitor
            .insertions
            .into_iter()
            .map(|offset| PaddingEdit::InsertLine { offset })
            .chain(
                visitor
                    .removals
                    .into_iter()
                    .map(|(start, end)| PaddingEdit::RemoveLine { range: start..end }),
            )
            .collect();
        edits.sort_unstable_by_key(PaddingEdit::start);
        let mut previous_offset = 0;
        let mut original_line = 1;
        let mut lines = Vec::with_capacity(edits.len());

        for edit in edits {
            let segment = &self.source[previous_offset..edit.start()];
            original_line += segment.bytes().filter(|byte| *byte == b'\n').count();
            output.push_str(segment);
            output.push_str(edit.replacement(self.source));
            lines.push(original_line);
            original_line += self.source[edit.start()..edit.end()].bytes().filter(|byte| *byte == b'\n').count();
            previous_offset = edit.end();
        }

        output.push_str(&self.source[previous_offset..]);
        Padding { output, lines }
    }
}

/// A whitespace-only replacement and the original line numbers needing padding changes.
pub struct Padding {
    /// Complete source with missing padding inserted and redundant inner padding removed.
    output: String,
    /// Original one-based lines needing an insertion or removal.
    lines: Vec<usize>,
}

/// An insertion or complete whitespace-only line removal in original source coordinates.
///
/// Sibling gaps and body-edge gaps are disjoint. Compacted bodies never
/// request inner insertions, and each set deduplicates edits before ordering them.
enum PaddingEdit {
    /// Adds one line ending at a safe line start.
    InsertLine {
        /// Original byte offset before the following source line.
        offset: usize,
    },
    /// Removes one empty physical line outside comments and literals.
    RemoveLine {
        /// Entire original line, including whitespace and its LF/CRLF ending.
        range: Range<usize>,
    },
}

impl PaddingEdit {
    /// Returns the first original byte affected by this change.
    fn start(&self) -> usize {
        match self {
            Self::InsertLine { offset } => *offset,
            Self::RemoveLine { range } => range.start,
        }
    }

    /// Returns the first original byte retained after this change.
    fn end(&self) -> usize {
        match self {
            Self::InsertLine { offset } => *offset,
            Self::RemoveLine { range } => range.end,
        }
    }

    /// Selects the neighboring line ending for insertions and no bytes for removals.
    fn replacement(&self, source: &str) -> &'static str {
        match self {
            Self::InsertLine { offset } if source[..*offset].ends_with("\r\n") => "\r\n",
            Self::InsertLine { .. } => "\n",
            Self::RemoveLine { .. } => "",
        }
    }
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

/// A full macro path whose parser-normalized spelling defines one visual group.
#[derive(PartialEq, Eq)]
struct MacroPath {
    /// Rust path tokens, retaining qualification and raw identifiers while ignoring trivia.
    spelling: String,
}

impl MacroPath {
    /// Captures the identity of an already parsed macro path without resolving aliases.
    fn new(path: &syn::Path) -> Self {
        Self {
            spelling: path.to_token_stream().to_string(),
        }
    }
}

/// Consecutive statements that share a visual purpose in one executable body.
#[derive(PartialEq, Eq)]
enum StatementGroup {
    /// Local `let` bindings, independent of their initializer expressions.
    Binding,
    /// Local item declarations, including constants, statics, types, and functions.
    Item,
    /// Ordinary expressions, calls, control flow, and assignments.
    Expression,
    /// Standalone invocations of one full macro path.
    Macro(MacroPath),
}

impl StatementGroup {
    /// Classifies the containing statement without looking inside initializer or macro tokens.
    fn for_statement(statement: &syn::Stmt) -> Self {
        match statement {
            syn::Stmt::Local(_) => Self::Binding,
            syn::Stmt::Item(_) => Self::Item,
            syn::Stmt::Expr(expression, _) => Self::for_expression(expression),
            syn::Stmt::Macro(statement) => Self::Macro(MacroPath::new(&statement.mac.path)),
        }
    }

    /// Recognizes plain, parenthesized, and tail macro expressions as the same macro group.
    fn for_expression(expression: &syn::Expr) -> Self {
        match expression {
            syn::Expr::Macro(expression) => Self::Macro(MacroPath::new(&expression.mac.path)),
            syn::Expr::Paren(expression) => Self::for_expression(&expression.expr),
            syn::Expr::Group(expression) => Self::for_expression(&expression.expr),
            _ => Self::Expression,
        }
    }
}

/// Collects disjoint, deduplicated padding edits without rewriting Rust tokens.
struct PaddingVisitor<'source> {
    /// Original file used to distinguish empty lines from lines inside comments.
    source: &'source str,
    /// Safe line starts needing exactly one extra line ending.
    insertions: BTreeSet<usize>,
    /// Complete empty lines removed only from compacted body-edge gaps.
    removals: BTreeSet<(usize, usize)>,
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
            || (block.stmts.len() == 1
                && (matches!(first, syn::Stmt::Expr(..) | syn::Stmt::Macro(_))
                    || (first.span().start().line == first.span().end().line && !has_braces(first.to_token_stream()))))
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

    /// Removes inner padding around a lone expression, including a trailing semicolon or macro invocation.
    fn single_expression_body(&mut self, block: &syn::Block) {
        if matches!(block.stmts.as_slice(), [syn::Stmt::Expr(..) | syn::Stmt::Macro(_)]) {
            self.compact_body_edges(block);
        }
    }

    /// Removes body-edge padding while preserving gaps between statements and comment contents.
    fn compact_body_edges(&mut self, block: &syn::Block) {
        let opening = block.brace_token.span.open().byte_range().end + self.syntax_offset;
        let closing = block.brace_token.span.close().byte_range().start + self.syntax_offset;
        let (Some(first), Some(last)) = (block.stmts.first(), block.stmts.last()) else {
            self.remove_empty_lines(opening..closing);
            return;
        };

        self.remove_empty_lines(opening..first.span().byte_range().start + self.syntax_offset);
        self.remove_empty_lines(last.span().byte_range().end + self.syntax_offset..closing);
    }

    /// Removes complete empty lines in a whitespace/comment gap, preserving nested comment contents.
    ///
    /// The first partial line belongs to the preceding token and is never removed.
    /// Non-trivia syntax, such as an inner attribute, keeps the entire gap unchanged.
    fn remove_empty_lines(&mut self, range: Range<usize>) {
        let gap = self.source[range.clone()].as_bytes();
        let mut offset = 0;
        let mut comment_depth = 0;
        let mut line_has_content = true;
        let mut line_start = range.start;
        let mut removals = Vec::new();

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
                if comment_depth == 0 && !line_has_content {
                    removals.push((line_start, range.start + offset + 1));
                }

                line_has_content = comment_depth > 0;
                line_start = range.start + offset + 1;
                offset += 1;
            } else if comment_depth > 0 {
                line_has_content = true;
                offset += 1;
            } else if byte.is_ascii_whitespace() {
                offset += 1;
            } else {
                return;
            }
        }

        self.removals.extend(removals);
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

    /// Separates changes of statement group while keeping leading comments with the next group.
    fn statement_groups(&mut self, statements: &[syn::Stmt]) {
        let Some((first, remaining)) = statements.split_first() else {
            return;
        };
        let mut previous = first;
        let mut previous_group = StatementGroup::for_statement(first);

        for next in remaining {
            let next_group = StatementGroup::for_statement(next);
            let gap = previous.span().byte_range().end + self.syntax_offset..next.span().byte_range().start + self.syntax_offset;

            if previous_group != next_group
                && let Some(offset) = missing_gap(self.source, gap)
            {
                self.insertions.insert(offset);
            }

            previous = next;
            previous_group = next_group;
        }
    }
}

impl<'syntax> Visit<'syntax> for PaddingVisitor<'_> {
    /// Keeps standalone scoped statements compact while recursively formatting their contents.
    fn visit_stmt(&mut self, node: &'syntax syn::Stmt) {
        if let syn::Stmt::Expr(syn::Expr::Block(expression), _) = node {
            self.compact_body_edges(&expression.block);
            visit::visit_expr_block(self, expression);
        } else {
            visit::visit_stmt(self, node);
        }
    }

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

    /// Compacts single-expression body edges and pads neighboring constructs and statement groups.
    fn visit_block(&mut self, node: &'syntax syn::Block) {
        self.single_expression_body(node);
        self.sequence(&node.stmts);
        self.statement_groups(&node.stmts);
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
