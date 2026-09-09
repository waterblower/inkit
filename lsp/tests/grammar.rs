use ink_lsp::index::parser;
use std::{fs, path::Path};
use tree_sitter::{Node, Query, QueryCursor, StreamingIterator};

fn children(node: Node<'_>) -> Vec<String> {
    node.named_children(&mut node.walk())
        .map(|child| child.kind().to_owned())
        .collect()
}

#[test]
fn semantic_boundaries_and_newlines() {
    let mut parser = parser();
    for (text, expected) in [
        ("=== 起点 ===\n= 房间\n", vec!["knot", "stitch"]),
        ("VARIABLE is prose.\n", vec!["content"]),
        ("VAR score = -2\n", vec!["variable_declaration"]),
        ("{a || b}\n", vec!["content"]),
        ("* [Pick] -> hall.room(1)\n", vec!["choice"]),
    ] {
        let tree = parser.parse(text, None).unwrap();
        assert!(!tree.root_node().has_error(), "{text}");
        assert_eq!(children(tree.root_node()), expected);
    }
    let tree = parser.parse("{a || b}\n", None).unwrap();
    let expression = tree
        .root_node()
        .named_child(0)
        .unwrap()
        .named_child(0)
        .unwrap();
    assert_eq!(expression.kind(), "inline_expression");
    assert_eq!(
        expression.named_child(0).unwrap().kind(),
        "binary_expression"
    );
    let text = "* [Pick] -> hall.room(1)\n";
    assert_eq!(
        captures("(path (identifier) @name)", text),
        vec![
            ("name".into(), "hall".into()),
            ("name".into(), "room".into())
        ]
    );
    assert_eq!(
        captures("(arguments (number) @number)", text),
        vec![("number".into(), "1".into())]
    );
    for newline in ["\n", "\r\n"] {
        for final_newline in ["", newline] {
            let text = format!("=== 起点 ==={newline}你好。{final_newline}");
            let tree = parser.parse(text, None).unwrap();
            assert!(!tree.root_node().has_error());
            assert_eq!(children(tree.root_node()), vec!["knot", "content"]);
        }
    }
}

#[test]
fn unfinished_constructs_preserve_following_declarations() {
    let mut parser = parser();
    for unfinished in [
        "* [Unfinished",
        "{count +",
        "===",
        "VAR value =",
        "{flag:\nText.",
    ] {
        let text = format!("{unfinished}\n=== recovered ===\nSafe prose.\n");
        let tree = parser.parse(&text, None).unwrap();
        assert!(
            tree.root_node().has_error() || tree.root_node().to_sexp().contains("missing_brace")
        );
        assert!(
            tree.root_node()
                .named_children(&mut tree.root_node().walk())
                .any(|node| {
                    node.kind() == "knot"
                        && node
                            .child_by_field_name("name")
                            .is_some_and(|name| &text[name.byte_range()] == "recovered")
                }),
            "{unfinished}"
        );
    }
}

fn captures(source: &str, text: &str) -> Vec<(String, String)> {
    let mut parser = parser();
    let tree = parser.parse(text, None).unwrap();
    let query = Query::new(&parser.language().unwrap(), source).unwrap();
    let mut cursor = QueryCursor::new();
    let mut matches = cursor.matches(&query, tree.root_node(), text.as_bytes());
    let mut result = Vec::new();
    while let Some(matched) = matches.next() {
        for capture in matched.captures {
            result.push((
                query.capture_names()[capture.index as usize].to_owned(),
                text[capture.node.byte_range()].to_owned(),
            ));
        }
    }
    result
}

#[test]
fn zed_queries_preserve_outline_and_prose_boundaries() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let fixture =
        fs::read_to_string(root.join("tree-sitter-ink/test/fixtures/features.ink")).unwrap();
    let query = |name| fs::read_to_string(root.join("languages/ink").join(name)).unwrap();
    let outline = captures(&query("outline.scm"), &fixture);
    let names: Vec<_> = outline
        .iter()
        .filter(|(kind, _)| kind == "name")
        .map(|(_, name)| name.as_str())
        .collect();
    assert_eq!(names, vec!["start", "hall", "inside", "double"]);
    for name in ["brackets.scm", "highlights.scm"] {
        assert!(
            captures(
                &query(name),
                "Ordinary (parentheses), \"quotes\", and Chinese “引号”.\n"
            )
            .is_empty()
        );
    }
    let brackets = captures(&query("brackets.scm"), "VAR x = (2 + 3)\n* [Pick]\n");
    for kind in ["open", "close"] {
        assert_eq!(brackets.iter().filter(|(name, _)| name == kind).count(), 2);
    }
}
