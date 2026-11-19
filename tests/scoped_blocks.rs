//! Regression cases for compact standalone scope edges and preserved inner spacing.

use bclippy::padding::RustSource;
use proc_macro2::TokenStream;

/// Applies padding while checking tokens and repeated-run stability.
fn padded(source: &str) -> String {
    let output = RustSource::parse(source).unwrap().padded().source().to_owned();
    assert_eq!(
        source.parse::<TokenStream>().unwrap().to_string(),
        output.parse::<TokenStream>().unwrap().to_string()
    );
    assert_eq!(RustSource::parse(&output).unwrap().padded().lines(), []);
    output
}

#[test]
fn windows_scope_removes_edge_padding_and_keeps_the_nested_if_separate() {
    let source = include_str!("fixtures/windows_scope_padded.txt");
    let expected = include_str!("fixtures/windows_scope_compact.txt");

    assert_eq!(padded(source), expected);
    assert_eq!(padded(expected), expected);
}

#[test]
fn standalone_scopes_stay_compact_with_attributes_labels_and_semicolons() {
    for header in ["", "#[cfg(windows)]\n    ", "'scope: "] {
        for ending in ["", ";"] {
            let source = format!("fn example() {{\n    {header}{{\n\n        first();\n        second();\n\n    }}{ending}\n}}\n");
            let expected = format!("fn example() {{\n    {header}{{\n        first();\n        second();\n    }}{ending}\n}}\n");

            assert_eq!(padded(&source), expected, "scope: {header}, ending: {ending}");
            assert_eq!(padded(&expected), expected);
        }
    }
}

#[test]
fn scoped_edges_stay_compact_while_statement_groups_and_nested_loops_get_padding() {
    let source = "fn example() {\n    before();\n    #[cfg(windows)]\n    {\n\n        let value = next();\n        use_value(value);\n        loop {\n            first();\n            second();\n        }\n\n    }\n    after();\n}\n";
    let expected = "fn example() {\n    before();\n\n    #[cfg(windows)]\n    {\n        let value = next();\n\n        use_value(value);\n\n        loop {\n\n            first();\n            second();\n\n        }\n    }\n\n    after();\n}\n";

    assert_eq!(padded(source), expected);
}

#[test]
fn nested_scopes_remove_only_their_edge_lines() {
    let source =
        "fn example() {\n    {\n\n        {\n\n            first();\n            second();\n\n        }\n        third();\n\n    }\n}\n";
    let expected =
        "fn example() {\n    {\n        {\n            first();\n            second();\n        }\n\n        third();\n    }\n}\n";

    assert_eq!(padded(source), expected);
}

#[test]
fn empty_and_comment_only_scopes_keep_comments_and_compact_edges() {
    for source in [
        "fn example() {\n    {\n\n    }\n}\n",
        "fn example() {\n    {\n\n        // Only a comment.\n\n    }\n}\n",
        "fn example() {\n    {\n\n        /* first\n\n           /* nested */ last */\n\n    }\n}\n",
    ] {
        let expected = source.replace("    {\n\n", "    {\n").replace("\n\n    }", "\n    }");

        assert_eq!(padded(source), expected);
    }
}

#[test]
fn scope_compaction_preserves_nested_comment_and_raw_literal_contents() {
    let source = "fn example() {\n    { // scope header\n\n        /* leading\n\n           /* nested */ comment */\n        #[cfg(test)]\n        first(r#\"literal\n\ncontents\"#);\n        second(); /* trailing\n\n           /* nested */ comment */\n\n    }\n}\n";
    let expected = source
        .replace("// scope header\n\n", "// scope header\n")
        .replace("comment */\n\n    }", "comment */\n    }");

    assert_eq!(padded(source), expected);
}

#[test]
fn scope_compaction_preserves_crlf_unicode_and_original_diagnostic_lines() {
    for prefix in [
        "",
        "\u{feff}",
        "#!/usr/bin/env rust-script\r\n",
        "\u{feff}#!/usr/bin/env rust-script\r\n",
    ] {
        let source =
            format!("{prefix}fn café() {{\r\n    {{\r\n \t\r\n        first(\"你好\");\r\n        second();\r\n\r\n    }}\r\n}}\r\n");
        let expected = format!("{prefix}fn café() {{\r\n    {{\r\n        first(\"你好\");\r\n        second();\r\n    }}\r\n}}\r\n");
        let result = RustSource::parse(&source).unwrap().padded();
        let prefix_lines = prefix.bytes().filter(|byte| *byte == b'\n').count();

        assert_eq!(result.source(), expected);
        assert_eq!(result.lines(), [3 + prefix_lines, 6 + prefix_lines]);
        assert_eq!(RustSource::parse(result.source()).unwrap().padded().lines(), []);
    }
}
