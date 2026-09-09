use crate::{
    index::{Location, Occurrence, Position, Range, offset_at, position_at},
    workspace::{Workspace, resolve},
};
use serde_json::{Value, json};
use std::collections::BTreeMap;

fn identifier_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '.'
}
#[derive(Clone, Copy, PartialEq)]
enum Context {
    Flow,
    Expression,
}

// Incomplete block comments are ERROR nodes, so supplement syntax context
// with delimiter tracking. Literal strings are identified by the parse tree.
fn inside_comment(root: tree_sitter::Node, text: &str, end: usize) -> bool {
    let bytes = text.as_bytes();
    let (mut line, mut block) = (false, false);
    let mut i = 0;
    while i < end {
        if line {
            if bytes[i] == b'\n' {
                line = false;
            }
        } else if block {
            if bytes[i..end].starts_with(b"*/") {
                block = false;
                i += 1;
            }
        } else if bytes[i] == b'\\' {
            i += 1;
        } else if bytes[i..end].starts_with(b"/*") || bytes[i..end].starts_with(b"//") {
            let mut node = root.named_descendant_for_byte_range(i, i + 1);
            let mut literal = false;
            while let Some(current) = node {
                if current.kind() == "inline_expression" {
                    break;
                }
                if matches!(current.kind(), "string" | "string_content") {
                    literal = true;
                    break;
                }
                node = current.parent();
            }
            if !literal {
                block = bytes[i + 1] == b'*';
                line = bytes[i + 1] == b'/';
                i += 1;
            }
        }
        i += 1;
    }
    line || block
}

/// Parse a temporary identifier at the cursor, so incomplete arrows work while
/// comments, strings and narrative remain excluded. Never modify the real buffer.
fn context(
    parser: &mut tree_sitter::Parser,
    text: &str,
    start: usize,
    end: usize,
) -> Option<Context> {
    // Tags consume the rest of the narrative line, even when an unfinished
    // tag makes Tree-sitter recover an arrow as a sibling divert.
    let line = text[..start].rsplit('\n').next().unwrap_or("");
    for (i, c) in line.char_indices() {
        if c == '#' && line[..i].chars().rev().take_while(|c| *c == '\\').count() % 2 == 0 {
            return None;
        }
    }
    const MARKER: &str = "ink_completion_placeholder";
    let mut patched = format!("{}{}{}", &text[..start], MARKER, &text[end..]);
    // An inline expression may still be missing its closing brace while typing.
    let line_start = text[..start].rfind('\n').map_or(0, |p| p + 1);
    let before = &text[line_start..start];
    let after = text[end..].split('\n').next().unwrap_or("");
    if before.rfind('{') > before.rfind('}') && !after.contains('}') {
        patched.insert(start + MARKER.len(), '}');
    }
    let tree = parser.parse(&patched, None)?;
    if inside_comment(tree.root_node(), &patched, start) {
        return None;
    }
    let node = tree
        .root_node()
        .named_descendant_for_byte_range(start, start + MARKER.len())?;
    if node.kind() != "identifier" {
        return None;
    }
    let mut parent = node.parent();
    while let Some(p) = parent {
        if matches!(
            p.kind(),
            "comment" | "string_content" | "text" | "tag" | "include"
        ) {
            return None;
        }
        if p.child_by_field_name("name")
            .is_some_and(|name| name.id() == node.id())
            && matches!(
                p.kind(),
                "knot"
                    | "stitch"
                    | "function"
                    | "parameter"
                    | "variable_declaration"
                    | "constant_declaration"
                    | "list_declaration"
                    | "external_declaration"
                    | "label"
            )
        {
            return None;
        }
        if matches!(
            p.kind(),
            "divert_destination" | "thread" | "tunnel_return" | "divert_target"
        ) {
            return Some(Context::Flow);
        }
        if matches!(
            p.kind(),
            "arguments"
                | "inline_expression"
                | "logic"
                | "binary_expression"
                | "unary_expression"
                | "call"
        ) {
            return Some(Context::Expression);
        }
        parent = p.parent();
    }
    None
}
impl Workspace {
    pub fn completion(&mut self, uri: &str, position: Position) -> Value {
        let empty = || json!({"isIncomplete": false, "items": []});
        let Some(text) = self.text(uri) else {
            return empty();
        };
        let byte = offset_at(&text, position);
        let start = text[..byte]
            .char_indices()
            .rev()
            .find(|(_, c)| !identifier_char(*c))
            .map_or(0, |(i, c)| i + c.len_utf8());
        let end = text[byte..]
            .char_indices()
            .find(|(_, c)| !identifier_char(*c))
            .map_or(text.len(), |(i, _)| byte + i);
        let Some(context) = context(&mut self.parser, &text, start, end) else {
            return empty();
        };
        self.refresh(uri);
        let Some(document) = self.documents.get(uri) else {
            return empty();
        };
        let scope = document.scope_at(byte);
        let range = Range {
            start: position_at(&text, start),
            end: position_at(&text, end),
        };
        let documents = self.component(uri);
        let mut items = BTreeMap::new();
        let prefix = text[start..byte].to_lowercase();
        let probe = Occurrence {
            name: String::new(),
            start: byte,
            end: byte,
            definition: None,
            scope: scope.clone(),
            location: Location {
                uri: uri.to_owned(),
                range,
            },
        };
        for symbol in documents.iter().flat_map(|d| &d.symbols) {
            if context == Context::Flow
                && !matches!(symbol.kind, "knot" | "stitch" | "label")
                && !symbol.divert
            {
                continue;
            }
            let mut names = vec![symbol.name.clone()];
            if symbol.qualified != symbol.name && !matches!(symbol.kind, "parameter" | "temp") {
                names.push(symbol.qualified.clone());
                if let Some(knot) = &scope.knot_name
                    && let Some(relative) = symbol.qualified.strip_prefix(&format!("{knot}."))
                {
                    names.push(relative.to_owned());
                }
            }
            for name in names {
                if !name.to_lowercase().starts_with(&prefix) {
                    continue;
                }
                let occurrence = Occurrence {
                    name: name.clone(),
                    ..probe.clone()
                };
                if !resolve(&occurrence, &documents)
                    .iter()
                    .any(|candidate| candidate.id == symbol.id)
                {
                    continue;
                }
                let local = symbol.owner.is_some() && name == symbol.name;
                let kind = match symbol.kind {
                    "function" | "external_declaration" => 3,
                    "constant_declaration" => 21,
                    "list_item" => 20,
                    "knot" | "stitch" | "label" => 18,
                    _ => 6,
                };
                items.entry(name.clone()).or_insert_with(|| json!({
                    "label": name, "kind": kind,
                    "detail": format!("{} · {}", symbol.kind.replace('_', " "), symbol.qualified),
                    "sortText": format!("{}{}", if local { "0" } else { "1" }, name),
                    "filterText": name, "textEdit": {"range": range, "newText": name}
                }));
            }
        }
        let builtins: &[&str] = if context == Context::Flow {
            &["END", "DONE"]
        } else {
            &["true", "false"]
        };
        for name in builtins {
            if name.to_lowercase().starts_with(&prefix) {
                items.entry((*name).to_owned()).or_insert_with(|| {
                    json!({"label": name, "kind": 14,
                    "detail": "Ink builtin", "sortText": format!("2{name}"), "filterText": name,
                    "textEdit": {"range": range, "newText": name}})
                });
            }
        }
        // Prefix-filtered results must be refreshed when subsequent characters change.
        json!({"isIncomplete": true, "items": items.into_values().collect::<Vec<_>>()})
    }
}
