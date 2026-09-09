use crate::{
    index::{Document, Position, offset_at, parser, position_at},
    workspace::{Workspace, file_path},
};
use base64::{Engine, engine::general_purpose::STANDARD};
use image::{ImageFormat, ImageReader};
use serde_json::{Value, json};
use std::{
    fs::File,
    io::{Cursor, Read},
    path::{Path, PathBuf},
};

const MAX_SOURCE: u64 = 20 * 1024 * 1024;

impl Workspace {
    pub fn hover(&mut self, uri: &str, position: Position) -> Value {
        let Some(text) = self.text(uri) else {
            return Value::Null;
        };
        let document = Document::new(&mut parser(), uri.to_owned(), text);
        let byte = offset_at(&document.text, position);
        let Some((start, end)) = image_path(&document, byte) else {
            return Value::Null;
        };
        let value = &document.text[start..end];
        let result = resolve_path(uri, value)
            .and_then(|path| preview(&path))
            .and_then(embedded_preview);
        let markdown = match result {
            Ok(markdown) => markdown,
            Err(error) => format!("Image preview unavailable: {error}"),
        };
        json!({"contents":{"kind":"markdown", "value":markdown}, "range":{
            "start":position_at(&document.text,start), "end":position_at(&document.text,end)}})
    }
}

// Keep hover payloads small even for photos or noisy images. Large inline URLs
// are fragile across editor Markdown implementations and expensive to render.
const MAX_PREVIEW_BYTES: usize = 6_000;

fn embedded_preview(png: Vec<u8>) -> Result<String, String> {
    let mut bytes = png;
    let mut mime = "png";
    if bytes.len() > MAX_PREVIEW_BYTES {
        let mut image =
            image::load_from_memory(&bytes).map_err(|_| "could not decode thumbnail.")?;
        let opaque = image.to_rgba8().pixels().all(|pixel| pixel[3] == 255);
        loop {
            let mut encoded = Cursor::new(Vec::new());
            if opaque {
                mime = "jpeg";
                image::codecs::jpeg::JpegEncoder::new_with_quality(&mut encoded, 70)
                    .encode_image(&image.to_rgb8())
                    .map_err(|_| "could not encode thumbnail.")?;
            } else {
                image
                    .write_to(&mut encoded, ImageFormat::Png)
                    .map_err(|_| "could not encode thumbnail.")?;
            }
            bytes = encoded.into_inner();
            if bytes.len() <= MAX_PREVIEW_BYTES {
                break;
            }
            image = image.thumbnail(
                (image.width() * 3 / 4).max(1),
                (image.height() * 3 / 4).max(1),
            );
        }
    }
    Ok(format!(
        "![Image preview](data:image/{mime};base64,{})",
        STANDARD.encode(bytes)
    ))
}

fn image_path(document: &Document, byte: usize) -> Option<(usize, usize)> {
    let mut stack = vec![document.tree.root_node()];
    let mut found = None;
    while let Some(node) = stack.pop() {
        if node.kind() == "tag" && node.start_byte() <= byte {
            let mut end = node.end_byte();
            let prefix = &document.text[node.start_byte()..end];
            // Ink sees the slashes in a file URL as a comment. Recover the
            // remainder only for a literal image tag starting with file:.
            let value = prefix.strip_prefix('#').unwrap_or("").trim_start();
            if value.strip_prefix("image:").is_some_and(|v| {
                v.trim_start()
                    .trim_start_matches(['\"', '\''])
                    .starts_with("file:")
            }) {
                end = document.text[node.start_byte()..]
                    .find('\n')
                    .map_or(document.text.len(), |n| node.start_byte() + n);
            }
            if byte < end {
                found = Some((node, end));
                break;
            }
        }
        let mut cursor = node.walk();
        stack.extend(node.named_children(&mut cursor));
    }
    let (node, end) = found?;
    let mut cursor = node.walk();
    if node
        .named_children(&mut cursor)
        // Backslashes in Windows file paths are Ink escape nodes, but remain
        // literal path characters here. Expressions still aren't previewed.
        .any(|child| !matches!(child.kind(), "text" | "escape"))
    {
        return None;
    }
    let raw = &document.text[node.start_byte()..end];
    let value = raw
        .strip_prefix('#')?
        .trim_start()
        .strip_prefix("image:")?
        .trim();
    let value = if (value.starts_with('"') && value.ends_with('"'))
        || (value.starts_with('\'') && value.ends_with('\''))
    {
        value.get(1..value.len().checked_sub(1)?)?
    } else {
        value
    };
    if value.is_empty() {
        return None;
    }
    let start = value.as_ptr() as usize - document.text.as_ptr() as usize;
    let end = start + value.len();
    (start <= byte && byte < end).then_some((start, end))
}

fn resolve_path(uri: &str, value: &str) -> Result<PathBuf, String> {
    if value.starts_with("file:") {
        return file_path(value).ok_or_else(|| "invalid file URL.".into());
    }
    if value.contains("://") {
        return Err("only local files are supported.".into());
    }
    let path = PathBuf::from(value);
    if path.is_absolute() {
        return Ok(path);
    }
    let source = file_path(uri).ok_or("document has no local directory.")?;
    Ok(source
        .parent()
        .ok_or("document has no local directory.")?
        .join(path))
}

fn preview(path: &Path) -> Result<Vec<u8>, String> {
    let file = File::open(path).map_err(|_| "file could not be opened.")?;
    let metadata = file
        .metadata()
        .map_err(|_| "file metadata could not be read.")?;
    if !metadata.is_file() {
        return Err("path is not a regular file.".into());
    }
    if metadata.len() > MAX_SOURCE {
        return Err("file exceeds 20 MiB.".into());
    }
    let mut bytes = Vec::new();
    file.take(MAX_SOURCE + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "file could not be read.")?;
    if bytes.len() as u64 > MAX_SOURCE {
        return Err("file exceeds 20 MiB.".into());
    }
    if path
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("svg"))
    {
        return svg_preview(&bytes);
    }
    let mut reader = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|_| "unknown image format.")?;
    let mut limits = image::Limits::default();
    limits.max_alloc = Some(256 * 1024 * 1024);
    reader.limits(limits);
    let decoded = reader
        .decode()
        .map_err(|e| format!("unsupported, corrupt, or oversized image: {e}"))?;
    let (width, height, _) = preview_size(decoded.width() as f32, decoded.height() as f32, false);
    let thumbnail = decoded.thumbnail(width, height);
    let mut png = Cursor::new(Vec::new());
    thumbnail
        .write_to(&mut png, ImageFormat::Png)
        .map_err(|_| "could not encode preview.")?;
    Ok(png.into_inner())
}

fn svg_preview(bytes: &[u8]) -> Result<Vec<u8>, String> {
    let mut options = resvg::usvg::Options::default();
    // Never resolve external SVG image resources, including local paths.
    options.image_href_resolver.resolve_string = Box::new(|_, _| None);
    options.fontdb_mut().load_system_fonts();
    let tree = resvg::usvg::Tree::from_data(bytes, &options).map_err(|_| "invalid SVG image.")?;
    let size = tree.size();
    let (width, height, scale) = preview_size(size.width(), size.height(), true);
    let mut pixmap =
        resvg::tiny_skia::Pixmap::new(width, height).ok_or("invalid SVG dimensions.")?;
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    pixmap
        .encode_png()
        .map_err(|_| "could not encode SVG preview.".into())
}

// Leave room for padding in Zed's hover popup. LSP does not expose its bounds.
fn preview_size(width: f32, height: f32, enlarge: bool) -> (u32, u32, f32) {
    let scale = (320.0 / width).min(200.0 / height);
    let scale = if enlarge { scale } else { scale.min(1.0) };
    // Round up so the SVG canvas contains the complete scaled image.
    let w = (width * scale).ceil().clamp(1.0, 320.0) as u32;
    let h = (height * scale).ceil().clamp(1.0, 200.0) as u32;
    (w, h, scale)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn windows_paths_are_literal_image_paths() {
        let text = "# image: \"C:\\stories\\images\\door.svg\"\n";
        let document = Document::new(&mut parser(), "file:///story.ink".into(), text.into());
        let (start, end) = image_path(&document, 12).unwrap();
        assert_eq!(&text[start..end], "C:\\stories\\images\\door.svg");
    }
    #[test]
    fn preview_bounds_contain_all_aspect_ratios() {
        for (width, height) in [
            (1600.0, 900.0),
            (900.0, 1600.0),
            (1000.0, 1000.0),
            (10.0, 10000.0),
        ] {
            for enlarge in [false, true] {
                let (w, h, scale) = preview_size(width, height, enlarge);
                assert!(w <= 320 && h <= 200);
                assert!((w as f32 - width * scale).abs() < 1.01);
                assert!((h as f32 - height * scale).abs() < 1.01);
            }
        }
        assert_eq!(preview_size(32.0, 16.0, false), (32, 16, 1.0));
    }
}
