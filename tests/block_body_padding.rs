//! User examples and boundary regressions for padding inside complex Rust blocks.

use bclippy::padding::RustSource;
use proc_macro2::TokenStream;

/// Applies padding while checking token preservation and repeated-run stability.
fn padded(source: &str) -> String {
    let output = RustSource::parse(source).unwrap().padded().source().to_owned();
    assert_eq!(
        source.parse::<TokenStream>().unwrap().to_string(),
        output.parse::<TokenStream>().unwrap().to_string()
    );
    assert!(RustSource::parse(&output).unwrap().padded().lines().is_empty());
    output
}

#[test]
fn nested_outbox_loops_get_inner_padding_without_rewriting_assertion_macros() {
    let source = include_str!("fixtures/urgency_unpadded.txt");
    let expected = include_str!("fixtures/urgency_padded.txt");

    assert_eq!(padded(source), expected);
}

#[test]
fn timeout_async_and_loop_bodies_get_padding_while_tiny_if_stays_compact() {
    let source = include_str!("fixtures/timeout_unpadded.txt");
    let expected = include_str!("fixtures/timeout_padded.txt");

    assert_eq!(padded(source), expected);
}

#[test]
fn conditional_bodies_with_several_statements_get_inner_padding() {
    let source = "fn example() {\n    if ready {\n        prepare();\n        finish();\n    } else {\n        reset();\n        retry();\n    }\n}\n";
    let expected = "fn example() {\n    if ready {\n\n        prepare();\n        finish();\n\n    } else {\n\n        reset();\n        retry();\n\n    }\n}\n";

    assert_eq!(padded(source), expected);
}

#[test]
fn complex_bodies_are_padded_across_executable_block_kinds() {
    for block in [
        "while ready {\n        first();\n        second();\n    }",
        "while let Some(value) = next() {\n        first();\n        second();\n    }",
        "for value in values {\n        first();\n        second();\n    }",
        "unsafe {\n        first();\n        second();\n    }",
        "let result = const {\n        first();\n        second()\n    };",
        "let result = async move {\n        first();\n        second().await\n    };",
        "let result = move || {\n        first();\n        second()\n    };",
        "let result = try {\n        first()?;\n        second()?\n    };",
        "'scope: {\n        first();\n        break 'scope second();\n    }",
        "let result = {\n        first();\n        second()\n    };",
    ] {
        let source = format!("fn example() {{\n    {block}\n}}\n");
        let expected = source
            .replace("        first(", "\n        first(")
            .replace("    }\n}", "\n    }\n}")
            .replace("    };\n}", "\n    };\n}");

        assert_eq!(padded(&source), expected, "block: {block}");
    }
}

#[test]
fn complex_match_arm_bodies_get_padding_without_separating_patterns_or_commas() {
    let source = "fn example() {\n    match value {\n        Some(value) => {\n            record(value);\n            finish();\n        },\n        None => {\n            return;\n        },\n    }\n}\n";
    let expected = source
        .replace("            record(", "\n            record(")
        .replace("        },\n        None", "\n        },\n\n        None");

    assert_eq!(padded(source), expected);
}

#[test]
fn a_single_statement_spanning_several_lines_gets_inner_padding() {
    let source = "fn example() {\n    if ready {\n        process(\n            first,\n            second,\n        );\n    }\n}\n";
    let expected = source
        .replace("        process(", "\n        process(")
        .replace("    }\n}", "\n    }\n}");

    assert_eq!(padded(source), expected);
}

#[test]
fn tiny_single_statement_bodies_remain_compact() {
    for block in [
        "if ready {\n        return;\n    }",
        "for value in values {\n        process(value);\n    }",
        "loop {\n        break;\n    }",
        "let result = async {\n        work().await\n    };",
        "if ready {\n        // A comment does not make a tiny statement complex.\n        return;\n    }",
    ] {
        let source = format!("fn example() {{\n    {block}\n}}\n");

        assert_eq!(padded(&source), source);
    }
}

#[test]
fn function_and_method_body_edges_stay_compact() {
    let source = "fn example() {\n    prepare();\n    finish();\n}\n\nimpl Example {\n    fn method(&self) {\n        prepare();\n        finish();\n    }\n}\n";

    assert_eq!(padded(source), source);
}

#[test]
fn comments_and_attributes_stay_inside_the_padded_body() {
    let source = "fn example() {\n    loop { // loop header\n        /* leading\n           /* nested */ comment */\n        #[cfg(test)]\n        prepare(); // first statement\n        finish(); // last statement\n        /* trailing\n           /* nested */ comment */\n    }\n}\n";
    let expected = source
        .replace("        /* leading", "\n        /* leading")
        .replace("    }\n}", "\n    }\n}");

    assert_eq!(padded(source), expected);
}

#[test]
fn existing_inner_blank_lines_are_preserved_without_accumulating_more() {
    let source = "fn example() {\n    loop {\n\n\n        first();\n        second();\n\n\n    }\n}\n";

    assert_eq!(padded(source), source);
}

#[test]
fn empty_single_line_and_macro_blocks_are_not_padded_inside() {
    let source = "fn example() {\n    loop {}\n\n    if ready { first(); second(); }\n\n    opaque! {\n        first();\n        second();\n    }\n}\n";

    assert_eq!(padded(source), source);
}

#[test]
fn raw_literal_contents_are_preserved_when_the_surrounding_body_gets_padded() {
    let source = "fn example() {\n    loop {\n        let text = r#\"first {\n\n} last\"#;\n        consume(text);\n    }\n}\n";
    let expected = source
        .replace("        let text", "\n        let text")
        .replace("    }\n}", "\n    }\n}");

    assert_eq!(padded(source), expected);
}

#[test]
fn inner_padding_preserves_unicode_crlf_and_prefix_offsets() {
    for prefix in [
        "",
        "\u{feff}",
        "#!/usr/bin/env rust-script\r\n",
        "\u{feff}#!/usr/bin/env rust-script\r\n",
    ] {
        let source = format!("{prefix}fn café() {{\r\n    loop {{\r\n        first(\"你好\");\r\n        second();\r\n    }}\r\n}}\r\n");
        let expected = source
            .replace("        first(", "\r\n        first(")
            .replace("    }\r\n}", "\r\n    }\r\n}");
        let result = RustSource::parse(&source).unwrap().padded();
        let prefix_lines = prefix.bytes().filter(|byte| *byte == b'\n').count();

        assert_eq!(result.source(), expected);
        assert_eq!(result.lines(), [3 + prefix_lines, 5 + prefix_lines]);
        assert!(RustSource::parse(result.source()).unwrap().padded().lines().is_empty());
    }
}
