use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tree_sitter::{Node, Parser, Tree};

unsafe extern "C" {
    fn tree_sitter_ink() -> *const ();
}

pub fn parser() -> Parser {
    let mut parser = Parser::new();
    // The function is linked from our generated C parser by build.rs.
    let language = unsafe { tree_sitter_language::LanguageFn::from_raw(tree_sitter_ink) };
    parser
        .set_language(&language.into())
        .expect("Ink grammar ABI is supported");
    parser
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct Position {
    pub line: usize,
    pub character: usize,
}
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct Range {
    pub start: Position,
    pub end: Position,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct Location {
    pub uri: String,
    pub range: Range,
}
#[derive(Clone, Debug, Default)]
pub struct Scope {
    pub knot: Option<String>,
    pub stitch: Option<String>,
    pub knot_name: Option<String>,
}
#[derive(Clone, Debug)]
pub struct Symbol {
    pub id: String,
    pub name: String,
    pub qualified: String,
    pub kind: &'static str,
    pub owner: Option<String>,
    pub start: usize,
    pub location: Location,
    pub divert: bool,
}
#[derive(Clone, Debug)]
pub struct Occurrence {
    pub name: String,
    pub start: usize,
    pub end: usize,
    pub definition: Option<String>,
    pub scope: Scope,
    pub location: Location,
}
#[derive(Clone, Debug)]
pub struct Include {
    pub path: String,
    pub start: usize,
    pub end: usize,
}
pub struct Document {
    pub uri: String,
    pub text: String,
    pub tree: Tree,
    pub symbols: Vec<Symbol>,
    pub occurrences: Vec<Occurrence>,
    pub includes: Vec<Include>,
    pub scopes: Vec<(usize, Scope)>,
}

/// Tree-sitter uses UTF-8 byte offsets; LSP positions use UTF-16 code units.
pub fn position_at(text: &str, mut byte: usize) -> Position {
    byte = byte.min(text.len());
    while !text.is_char_boundary(byte) {
        byte -= 1;
    }
    let prefix = &text[..byte];
    let start = prefix.rfind('\n').map_or(0, |p| p + 1);
    Position {
        line: prefix.bytes().filter(|b| *b == b'\n').count(),
        character: text[start..byte].encode_utf16().count(),
    }
}
pub fn offset_at(text: &str, position: Position) -> usize {
    let mut start = 0;
    for _ in 0..position.line {
        let Some(next) = text[start..].find('\n') else {
            return text.len();
        };
        start += next + 1;
    }
    let mut units = 0;
    for (offset, c) in text[start..].char_indices() {
        if c == '\n' || c == '\r' || units + c.len_utf16() > position.character {
            return start + offset;
        }
        units += c.len_utf16();
        if units == position.character {
            return start + offset + c.len_utf8();
        }
    }
    text.len()
}
fn node_text<'a>(text: &'a str, node: Node) -> &'a str {
    &text[node.byte_range()]
}
fn node_range(text: &str, node: Node) -> Range {
    Range {
        start: position_at(text, node.start_byte()),
        end: position_at(text, node.end_byte()),
    }
}

impl Document {
    pub fn new(parser: &mut Parser, uri: String, text: String) -> Self {
        let tree = parser
            .parse(&text, None)
            .expect("parser has no cancellation or timeout");
        let mut symbols = Vec::new();
        let mut occurrences = Vec::new();
        let mut includes = Vec::new();
        let mut scopes = Vec::new();
        let mut scope = Scope::default();
        let mut declarations = HashMap::new();
        // Iterative preorder traversal also indexes recoverable nodes inside ERROR.
        let mut stack = vec![tree.root_node()];
        while let Some(node) = stack.pop() {
            let name = node.child_by_field_name("name");
            let kind = node.kind();
            let owner = match kind {
                "parameter" => node
                    .parent()
                    .and_then(|p| p.parent())
                    .filter(|p| p.kind() == "external_declaration")
                    .and_then(|p| p.child_by_field_name("name"))
                    .map(|name| format!("{uri}#{}", name.start_byte()))
                    .or_else(|| scope.stitch.clone().or(scope.knot.clone())),
                "label" => scope.stitch.clone().or(scope.knot.clone()),
                "stitch" => scope.knot.clone(),
                "assignment" => scope
                    .stitch
                    .clone()
                    .or(scope.knot.clone())
                    .or(Some(format!("{uri}#top"))),
                _ => None,
            };
            let mut cursor = node.walk();
            let is_temp =
                kind == "assignment" && node.children(&mut cursor).any(|c| c.kind() == "temp");
            let is_declaration = matches!(
                kind,
                "knot"
                    | "function"
                    | "stitch"
                    | "parameter"
                    | "label"
                    | "variable_declaration"
                    | "constant_declaration"
                    | "list_declaration"
                    | "external_declaration"
                    | "list_item"
            ) || is_temp;
            if is_declaration && let Some(name) = name.filter(|n| !n.is_missing()) {
                let spelling = node_text(&text, name).to_owned();
                let id = format!("{uri}#{}", name.start_byte());
                let qualified = match kind {
                    "stitch" => scope
                        .knot_name
                        .as_ref()
                        .map_or(spelling.clone(), |k| format!("{k}.{spelling}")),
                    "label" => {
                        let parent = symbols.iter().find(|s: &&Symbol| {
                            Some(&s.id) == scope.stitch.as_ref().or(scope.knot.as_ref())
                        });
                        parent.map_or(spelling.clone(), |p| format!("{}.{spelling}", p.qualified))
                    }
                    "list_item" => node
                        .parent()
                        .and_then(|p| p.child_by_field_name("name"))
                        .map_or(spelling.clone(), |p| {
                            format!("{}.{spelling}", node_text(&text, p))
                        }),
                    _ => spelling.clone(),
                };
                let mut cursor = node.walk();
                let divert = node
                    .children(&mut cursor)
                    .any(|c| matches!(c.kind(), "->" | "divert_target"));
                declarations.insert(name.start_byte(), id.clone());
                symbols.push(Symbol {
                    id: id.clone(),
                    name: spelling.clone(),
                    qualified,
                    kind: if is_temp { "temp" } else { kind },
                    owner,
                    start: name.start_byte(),
                    location: Location {
                        uri: uri.clone(),
                        range: node_range(&text, name),
                    },
                    divert,
                });
                if matches!(kind, "knot" | "function") {
                    scope = Scope {
                        knot: Some(id),
                        stitch: None,
                        knot_name: Some(spelling),
                    };
                    scopes.push((node.start_byte(), scope.clone()));
                } else if kind == "stitch" {
                    scope.stitch = Some(id);
                    scopes.push((node.start_byte(), scope.clone()));
                }
            }
            if kind == "include" {
                if let Some(path) = node.child_by_field_name("path") {
                    includes.push(Include {
                        path: node_text(&text, path)
                            .split("//")
                            .next()
                            .unwrap_or("")
                            .trim()
                            .to_owned(),
                        start: path.start_byte(),
                        end: path.end_byte(),
                    });
                }
                continue;
            }
            if kind == "identifier" && !node.is_missing() {
                let spelling = if let Some(parent) = node
                    .parent()
                    .filter(|p| matches!(p.kind(), "path" | "path_expression"))
                {
                    let mut cursor = parent.walk();
                    parent
                        .named_children(&mut cursor)
                        .filter(|c| c.kind() == "identifier" && c.start_byte() <= node.start_byte())
                        .map(|c| node_text(&text, c))
                        .collect::<Vec<_>>()
                        .join(".")
                } else {
                    node_text(&text, node).to_owned()
                };
                occurrences.push(Occurrence {
                    name: spelling,
                    start: node.start_byte(),
                    end: node.end_byte(),
                    definition: declarations.get(&node.start_byte()).cloned(),
                    scope: scope.clone(),
                    location: Location {
                        uri: uri.clone(),
                        range: node_range(&text, node),
                    },
                });
            }
            let mut cursor = node.walk();
            let children: Vec<_> = node.named_children(&mut cursor).collect();
            stack.extend(children.into_iter().rev());
        }
        Self {
            uri,
            text,
            tree,
            symbols,
            occurrences,
            includes,
            scopes,
        }
    }
    pub fn occurrence_at(&self, position: Position) -> Option<&Occurrence> {
        let byte = offset_at(&self.text, position);
        self.occurrences
            .iter()
            .find(|o| o.start <= byte && byte < o.end)
            .or_else(|| self.occurrences.iter().find(|o| o.end == byte))
    }
    pub fn scope_at(&self, byte: usize) -> Scope {
        self.scopes
            .iter()
            .rev()
            .find(|(start, _)| *start <= byte)
            .map(|(_, scope)| scope.clone())
            .unwrap_or_default()
    }
}
