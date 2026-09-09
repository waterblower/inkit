use crate::index::{self, Document, Location, Occurrence, Position, Range, Symbol, offset_at};
use serde::Deserialize;
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    fs,
    path::{Path, PathBuf},
};
use url::Url;

#[derive(Deserialize)]
pub struct Change {
    pub range: Option<Range>,
    pub text: String,
}
pub struct Buffer {
    pub text: String,
    pub version: i64,
}
pub struct Workspace {
    pub parser: tree_sitter::Parser,
    pub roots: Vec<String>,
    pub open: BTreeMap<String, Buffer>,
    pub documents: BTreeMap<String, Document>,
    edges: BTreeMap<String, BTreeSet<String>>,
}
impl Default for Workspace {
    fn default() -> Self {
        Self {
            parser: index::parser(),
            roots: Vec::new(),
            open: BTreeMap::new(),
            documents: BTreeMap::new(),
            edges: BTreeMap::new(),
        }
    }
}
pub fn file_path(uri: &str) -> Option<PathBuf> {
    Url::parse(uri).ok()?.to_file_path().ok()
}
pub fn file_uri(path: &Path) -> Option<String> {
    Url::from_file_path(path).ok().map(Into::into)
}
pub fn include_uri(uri: &str, include: &str) -> Option<String> {
    let file = file_path(uri)?;
    let joined = file.parent()?.join(include);
    let mut normalized = PathBuf::new();
    for part in joined.components() {
        match part {
            std::path::Component::ParentDir => {
                normalized.pop();
            }
            std::path::Component::CurDir => {}
            _ => normalized.push(part),
        }
    }
    file_uri(&normalized)
}
fn files_under(directory: &Path, files: &mut BTreeSet<String>) {
    let mut pending = vec![directory.to_path_buf()];
    while let Some(directory) = pending.pop() {
        let Ok(entries) = fs::read_dir(directory) else {
            continue;
        };
        for entry in entries.flatten() {
            if matches!(
                entry.file_name().to_str(),
                Some(
                    ".git"
                        | ".hg"
                        | ".svn"
                        | ".cache"
                        | ".local"
                        | "node_modules"
                        | "target"
                        | "grammars"
                )
            ) {
                continue;
            }
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            if kind.is_dir() {
                pending.push(entry.path());
            } else if kind.is_file()
                && entry.path().extension().is_some_and(|ext| ext == "ink")
                && let Some(uri) = file_uri(&entry.path())
            {
                files.insert(uri);
            }
        }
    }
}
impl Workspace {
    pub fn set_roots(&mut self, roots: Vec<String>) {
        self.roots = roots
            .into_iter()
            .filter(|r| file_path(r).is_some())
            .collect();
    }
    pub fn open_document(&mut self, uri: String, text: String, version: i64) {
        self.open.insert(uri, Buffer { text, version });
    }
    pub fn change_document(&mut self, uri: &str, changes: Vec<Change>, version: i64) {
        let Some(buffer) = self.open.get_mut(uri) else {
            return;
        };
        if version <= buffer.version {
            return;
        }
        for change in changes {
            if let Some(range) = change.range {
                let start = offset_at(&buffer.text, range.start);
                let end = offset_at(&buffer.text, range.end);
                if start <= end {
                    buffer.text.replace_range(start..end, &change.text);
                }
            } else {
                buffer.text = change.text;
            }
        }
        buffer.version = version;
    }
    pub fn close_document(&mut self, uri: &str) {
        self.open.remove(uri);
    }
    pub fn text(&self, uri: &str) -> Option<String> {
        self.open
            .get(uri)
            .map(|b| b.text.clone())
            .or_else(|| fs::read_to_string(file_path(uri)?).ok())
    }
    pub fn refresh(&mut self, request_uri: &str) {
        let mut files: BTreeSet<_> = self.open.keys().cloned().collect();
        files.insert(request_uri.to_owned());
        if self.roots.is_empty() {
            if let Some(file) = file_path(request_uri)
                && let Some(parent) = file.parent()
            {
                files_under(parent, &mut files);
            }
        } else {
            for root in &self.roots {
                if let Some(path) = file_path(root) {
                    files_under(&path, &mut files);
                }
            }
        }
        let mut pending: VecDeque<_> = files.into_iter().collect();
        let mut visited = BTreeSet::new();
        let mut documents = BTreeMap::new();
        let mut edges: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        while let Some(uri) = pending.pop_front() {
            if !visited.insert(uri.clone()) {
                continue;
            }
            let Some(text) = self.text(&uri) else {
                continue;
            };
            let document = match self.documents.remove(&uri) {
                Some(cached) if cached.text == text => cached,
                _ => Document::new(&mut self.parser, uri.clone(), text),
            };
            for include in &document.includes {
                if let Some(target) = include_uri(&uri, &include.path) {
                    edges.entry(uri.clone()).or_default().insert(target.clone());
                    edges.entry(target.clone()).or_default().insert(uri.clone());
                    pending.push_back(target);
                }
            }
            documents.insert(uri, document);
        }
        self.documents = documents;
        self.edges = edges;
    }
    pub fn component(&self, uri: &str) -> Vec<&Document> {
        let mut visited = BTreeSet::new();
        let mut pending = vec![uri.to_owned()];
        while let Some(uri) = pending.pop() {
            if !visited.insert(uri.clone()) {
                continue;
            }
            if let Some(edges) = self.edges.get(&uri) {
                pending.extend(edges.iter().cloned());
            }
        }
        visited
            .iter()
            .filter_map(|uri| self.documents.get(uri))
            .collect()
    }
    pub fn definitions(&mut self, uri: &str, position: Position) -> Vec<Location> {
        self.refresh(uri);
        let Some(document) = self.documents.get(uri) else {
            return Vec::new();
        };
        let byte = offset_at(&document.text, position);
        if let Some(include) = document
            .includes
            .iter()
            .find(|i| i.start <= byte && byte <= i.end)
        {
            return include_uri(uri, &include.path)
                .filter(|target| self.documents.contains_key(target))
                .map(|uri| {
                    vec![Location {
                        uri,
                        range: Range {
                            start: Position::default(),
                            end: Position::default(),
                        },
                    }]
                })
                .unwrap_or_default();
        }
        document
            .occurrence_at(position)
            .map(|o| {
                resolve(o, &self.component(uri))
                    .into_iter()
                    .map(|s| s.location.clone())
                    .collect()
            })
            .unwrap_or_default()
    }
    pub fn references(
        &mut self,
        uri: &str,
        position: Position,
        include_declaration: bool,
    ) -> Vec<Location> {
        self.refresh(uri);
        let Some(document) = self.documents.get(uri) else {
            return Vec::new();
        };
        let Some(occurrence) = document.occurrence_at(position) else {
            return Vec::new();
        };
        let documents = self.component(uri);
        let targets: BTreeSet<_> = resolve(occurrence, &documents)
            .iter()
            .map(|s| s.id.clone())
            .collect();
        documents
            .iter()
            .flat_map(|d| &d.occurrences)
            .filter(|o| include_declaration || o.definition.is_none())
            .filter(|o| {
                resolve(o, &documents)
                    .iter()
                    .any(|s| targets.contains(&s.id))
            })
            .map(|o| o.location.clone())
            .collect()
    }
}
pub fn resolve<'a>(occurrence: &Occurrence, documents: &[&'a Document]) -> Vec<&'a Symbol> {
    let symbols: Vec<_> = documents.iter().flat_map(|d| &d.symbols).collect();
    if let Some(id) = &occurrence.definition {
        return symbols.into_iter().filter(|s| &s.id == id).collect();
    }
    if matches!(occurrence.name.as_str(), "END" | "DONE") {
        return Vec::new();
    }
    let top = format!("{}#top", occurrence.location.uri);
    for owner in [
        occurrence.scope.stitch.as_ref(),
        occurrence.scope.knot.as_ref(),
        Some(&top),
    ]
    .into_iter()
    .flatten()
    {
        let local: Vec<_> = symbols
            .iter()
            .copied()
            .filter(|s| {
                s.name == occurrence.name
                    && s.owner.as_ref() == Some(owner)
                    && (s.kind != "temp" || s.start <= occurrence.start)
            })
            .collect();
        if !local.is_empty() {
            return local;
        }
    }
    if occurrence.name.contains('.') {
        let qualified: Vec<_> = symbols
            .iter()
            .copied()
            .filter(|s| s.qualified == occurrence.name && !matches!(s.kind, "parameter" | "temp"))
            .collect();
        if !qualified.is_empty() {
            return qualified;
        }
        if let Some(knot) = &occurrence.scope.knot_name {
            return symbols
                .into_iter()
                .filter(|s| s.qualified == format!("{knot}.{}", occurrence.name))
                .collect();
        }
        return Vec::new();
    }
    symbols
        .into_iter()
        .filter(|s| s.name == occurrence.name && s.owner.is_none())
        .collect()
}
