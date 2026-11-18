//! Visual statement grouping with token preservation and repeated-run stability.

use bclippy::padding::RustSource;
use proc_macro2::TokenStream;

/// Formats a real Rust source example without changing tokens or accumulating padding.
fn padded(source: &str) -> String {
    let result = RustSource::parse(source).unwrap().padded();
    let output = result.source();
    assert_eq!(
        source.parse::<TokenStream>().unwrap().to_string(),
        output.parse::<TokenStream>().unwrap().to_string()
    );
    assert_eq!(RustSource::parse(output).unwrap().padded().lines(), []);
    output.to_owned()
}

#[test]
fn favicon_example_groups_bindings_then_equality_checks_then_boolean_checks() {
    let source = include_str!("fixtures/grouping_ungrouped.txt");
    let expected = include_str!("fixtures/grouping_grouped.txt");

    assert_eq!(padded(source), expected);
    assert_eq!(padded(expected), expected);
}

#[test]
fn binding_call_and_macro_runs_are_separated_without_splitting_ordinary_calls() {
    let source = "fn example() {\n    let first = 1;\n    let second = 2;\n    process(first);\n    record(second);\n    let third = 3;\n    assert_eq!(first, second);\n    assert!(third > second);\n    finish();\n}\n";
    let expected = "fn example() {\n    let first = 1;\n    let second = 2;\n\n    process(first);\n    record(second);\n\n    let third = 3;\n\n    assert_eq!(first, second);\n\n    assert!(third > second);\n\n    finish();\n}\n";

    assert_eq!(padded(source), expected);
}

#[test]
fn macro_groups_use_the_full_path_and_keep_consecutive_matches_together() {
    let source =
        "fn example() {\n    first::check!(one);\n    first::check!(two);\n    second::check!(three);\n    ::first::check!(four);\n}\n";
    let expected =
        "fn example() {\n    first::check!(one);\n    first::check!(two);\n\n    second::check!(three);\n\n    ::first::check!(four);\n}\n";

    assert_eq!(padded(source), expected);
}

#[test]
fn macro_initializers_remain_in_the_binding_group() {
    let source = "fn example() {\n    let first = first_macro!();\n    let second = second_macro!();\n    let third = process();\n}\n";

    assert_eq!(padded(source), source);
}

#[test]
fn grouping_keeps_leading_comments_and_attributes_attached_to_the_next_statement() {
    let source = "fn example() {\n    let value = 1; // trailing\n    /* leading\n\n       /* nested */ comment */\n    #[cfg(test)]\n    assert_eq!(value, 1);\n    // explains the boolean check\n    assert!(value > 0);\n}\n";
    let expected = source
        .replace("    /* leading", "\n    /* leading")
        .replace("    // explains the boolean check", "\n    // explains the boolean check");

    assert_eq!(padded(source), expected);
}

#[test]
fn inline_blocks_are_left_for_rustfmt_to_expand_first() {
    let source = "fn example() { let value = 1; assert_eq!(value, 1); assert!(value > 0); }\n";

    assert_eq!(padded(source), source);
}

#[test]
fn opaque_macro_contents_and_literal_blank_lines_are_preserved() {
    let source = "fn example() {\n    let text = r#\"first\n\nlast\"#;\n    opaque! {\n        let value = 1;\n        assert_eq!(value, 1);\n        assert!(value > 0);\n    }\n    finish(text);\n}\n";
    let expected = source
        .replace("    opaque!", "\n    opaque!")
        .replace("    finish(text)", "\n    finish(text)");

    assert_eq!(padded(source), expected);
}

#[test]
fn local_items_form_a_group_while_top_level_items_and_struct_fields_keep_their_layout() {
    let source = "const FIRST: u8 = 1;\nstatic SECOND: u8 = 2;\nstruct Record { first: u8, second: u8 }\n\nfn example() {\n    const LOCAL: u8 = 1;\n    static OTHER: u8 = 2;\n    let record = Record { first: LOCAL, second: OTHER };\n    consume(record);\n}\n";
    let expected = source
        .replace("    let record", "\n    let record")
        .replace("    consume(record)", "\n    consume(record)");

    assert_eq!(padded(source), expected);
}

#[test]
fn parenthesized_and_tail_macro_expressions_share_their_macro_group() {
    let source =
        "fn example() {\n    assert_eq!(first, second);\n    (assert_eq!(third, fourth));\n    assert!(ready);\n    (assert!(done))\n}\n";
    let expected = source.replace("    assert!(ready)", "\n    assert!(ready)");

    assert_eq!(padded(source), expected);
}

#[test]
fn unicode_crlf_and_prefix_offsets_remain_original_in_group_diagnostics() {
    for prefix in [
        "",
        "\u{feff}",
        "#!/usr/bin/env rust-script\r\n",
        "\u{feff}#!/usr/bin/env rust-script\r\n",
    ] {
        let source = format!(
            "{prefix}fn café() {{\r\n    let value = \"你好\";\r\n    assert_eq!(value, \"你好\");\r\n    assert!(ready);\r\n}}\r\n"
        );
        let expected = source
            .replace("    assert_eq!", "\r\n    assert_eq!")
            .replace("    assert!(ready)", "\r\n    assert!(ready)");
        let result = RustSource::parse(&source).unwrap().padded();
        let prefix_lines = prefix.bytes().filter(|byte| *byte == b'\n').count();

        assert_eq!(result.source(), expected);
        assert_eq!(result.lines(), [3 + prefix_lines, 4 + prefix_lines]);
        assert_eq!(RustSource::parse(result.source()).unwrap().padded().lines(), []);
    }
}
