//! Regression examples for safe, idempotent multiline-block padding.

use bclippy::padding::RustSource;
use proc_macro2::TokenStream;

/// Applies the rule and verifies that only whitespace between tokens changes.
fn padded(source: &str) -> String {
    let output = RustSource::parse(source).unwrap().padded().source().to_owned();
    assert_eq!(
        source.parse::<TokenStream>().unwrap().to_string(),
        output.parse::<TokenStream>().unwrap().to_string()
    );
    assert!(
        RustSource::parse(&output).unwrap().padded().lines().is_empty(),
        "the fix must be idempotent"
    );
    output
}

#[test]
fn browser_bootstrap_gets_padding_on_both_sides_of_if() {
    let source = r#"fn read(input: &mut impl BufRead) -> Result<Self, Box<dyn std::error::Error>> {
    let mut bytes = Vec::new();
    input.take(65_537).read_until(b'\n', &mut bytes)?;
    if bytes.len() > 65_536 || bytes.last() != Some(&b'\n') {
        return Err("Invalid bounded browser bootstrap".into());
    }
    Ok(serde_json::from_slice(&bytes)?)
}
"#;
    let expected = source
        .replace("    if bytes.len()", "\n    if bytes.len()")
        .replace("    Ok(serde_json", "\n    Ok(serde_json");

    assert_eq!(padded(source), expected);
}

#[test]
fn all_multiline_brace_constructs_get_separated_from_neighboring_statements() {
    for block in [
        "if ready {\n        work();\n    }",
        "while ready {\n        work();\n    }",
        "while let Some(value) = next() {\n        work(value);\n    }",
        "for value in values {\n        work(value);\n    }",
        "loop {\n        break;\n    }",
        "match value {\n        _ => work(),\n    }",
        "unsafe {\n        work();\n    }",
        "let value = async {\n        work().await;\n    };",
        "let value = const {\n        1\n    };",
        "{\n        work();\n    }",
        "'scope: {\n        break 'scope;\n    }",
        "let value = if ready {\n        one()\n    } else {\n        two()\n    };",
        "let Some(value) = input else {\n        return;\n    };",
        "let value = || {\n        work();\n    };",
        "call(|| {\n        work();\n    });",
        "let value = Record {\n        field: 1,\n    };",
        "custom! {\n        arbitrary tokens\n    }",
    ] {
        let source = format!("fn example() {{\n    before();\n    {block}\n    after();\n}}\n");
        let expected = format!("fn example() {{\n    before();\n\n    {block}\n\n    after();\n}}\n");
        assert_eq!(padded(&source), expected, "block: {block}");
    }
}

#[test]
fn block_edges_and_single_line_blocks_need_no_padding() {
    for source in [
        "fn example() {\n    if ready {\n        work();\n    }\n}\n",
        "fn example() {\n    before();\n    if ready { work(); }\n    after();\n}\n",
        "fn example() {\n    before();\n    let text = \"{\\n}\";\n    after();\n}\n",
    ] {
        assert_eq!(padded(source), source);
    }
}

#[test]
fn else_chains_and_let_else_remain_connected() {
    let source = "fn example() {\n    before();\n    if first {\n        one();\n    } else if second {\n        two();\n    } else {\n        three();\n    }\n    let Some(value) = input else {\n        return;\n    };\n    after();\n}\n";
    let expected = source
        .replace("    if first", "\n    if first")
        .replace("    let Some", "\n    let Some")
        .replace("    after();", "\n    after();");

    assert_eq!(padded(source), expected);
}

#[test]
fn padding_stays_outside_attached_comments_and_attributes() {
    let source = "fn example() {\n    before(); // trailing comment\n    // explains the block\n    #[cfg(test)]\n    if ready {\n        work();\n    } // block comment\n    // explains the result\n    after();\n}\n";
    let expected = source
        .replace("    // explains the block", "\n    // explains the block")
        .replace("    // explains the result", "\n    // explains the result");

    assert_eq!(padded(source), expected);
}

#[test]
fn blank_lines_inside_nested_block_comments_do_not_count_as_padding() {
    let source = "fn example() {\n    before(); /* trailing\n\n       /* nested */ comment */\n    /* leading\n\n       comment */\n    if ready {\n        work();\n    }\n}\n";
    let expected = source.replace("    /* leading", "\n    /* leading");

    assert_eq!(padded(source), expected);
}

#[test]
fn raw_literals_and_macro_bodies_are_preserved_verbatim() {
    let source = "fn example() {\n    let text = r#\"literal {\n\n} contents\"#;\n    opaque! {\n        before();\n        if ready {\n            work();\n        }\n        after();\n    }\n    done();\n}\n";
    let expected = source
        .replace("    opaque!", "\n    opaque!")
        .replace("    done();", "\n    done();");

    assert_eq!(padded(source), expected);
}

#[test]
fn existing_padding_and_adjacent_blocks_do_not_accumulate_lines() {
    let source = "fn example() {\n    before();\n\n    if ready {\n        one();\n    }\n    while ready {\n        two();\n    }\n\n    after();\n}\n";
    let expected = source.replace("    while ready", "\n    while ready");

    assert_eq!(padded(source), expected);
}

#[test]
fn match_arms_items_and_nested_closure_arguments_are_padded() {
    let source = "const VALUE: u8 = 1;\nfn example() {\n    match value {\n        0 => {\n            first();\n        },\n        _ => second(),\n    }\n    call(\n        first,\n        || {\n            work();\n        },\n        last,\n    );\n}\nconst OTHER: u8 = 2;\n";
    let expected = source
        .replace("fn example()", "\nfn example()")
        .replace("        _ =>", "\n        _ =>")
        .replace("    call(", "\n    call(")
        .replace("const OTHER", "\nconst OTHER");

    assert_eq!(padded(source), expected);
}

#[test]
fn unicode_and_crlf_line_endings_are_preserved() {
    let source = "fn café() {\r\n    before(\"你好\");\r\n    if ready {\r\n        work();\r\n    }\r\n    after();\r\n}\r\n";
    let expected = source
        .replace("    if ready", "\r\n    if ready")
        .replace("    after();", "\r\n    after();");

    assert_eq!(padded(source), expected);
}

#[test]
fn invalid_source_is_rejected_without_a_replacement() {
    assert!(RustSource::parse("fn incomplete( {").is_err());
}

#[test]
fn executable_shebang_and_bom_do_not_shift_insertion_positions() {
    for prefix in ["\u{feff}", "#!/usr/bin/env rust-script\n", "\u{feff}#!/usr/bin/env rust-script\r\n"] {
        let body = "fn example() {\n    before();\n    if ready {\n        work();\n    }\n    after();\n}\n";
        let source = format!("{prefix}{body}");
        let expected = format!(
            "{prefix}{}",
            body.replace("    if ready", "\n    if ready")
                .replace("    after();", "\n    after();")
        );
        let result = RustSource::parse(&source).unwrap().padded();

        assert_eq!(result.source(), expected);
        assert_eq!(RustSource::parse(result.source()).unwrap().padded().lines(), []);
    }
}

#[test]
fn inner_padding_preserves_trailing_nested_comment_contents() {
    let source = "fn example() {\n    while ready { // body header\n        // explains the first step\n        begin();\n        finish(); /* trailing\n\n            /* nested */ comment */\n        // explains the final step\n    }\n}\n";
    let expected = source
        .replace("        // explains the first step", "\n        // explains the first step")
        .replace("        // explains the final step\n", "        // explains the final step\n\n");

    assert_eq!(padded(source), expected);
}

#[test]
fn inline_nested_braces_make_an_otherwise_single_line_body_complex() {
    let source = "fn example() {\n    for state in states {\n        assert_eq!(state, if completed { \"refused\" } else { \"pending\" });\n    }\n}\n";
    let expected = source
        .replace("        assert_eq!", "\n        assert_eq!")
        .replace("    }\n", "\n    }\n");

    assert_eq!(padded(source), expected);
}
