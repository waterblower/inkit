use ink_lsp::{
    index::{Position, offset_at, position_at},
    protocol::{read_frame, write_message},
    workspace::{Change, Workspace, file_uri},
};
use serde_json::{Value, json};
use std::{
    fs,
    io::{BufReader, Write},
    process::{Command, Stdio},
    sync::mpsc,
    time::Duration,
};

struct Fixture {
    directory: tempfile::TempDir,
    workspace: Workspace,
}
impl Fixture {
    fn new(files: &[(&str, &str)]) -> Self {
        let directory = tempfile::tempdir().unwrap();
        for (name, text) in files {
            let path = directory.path().join(name);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, text).unwrap();
        }
        let mut workspace = Workspace::default();
        workspace.set_roots(vec![file_uri(directory.path()).unwrap()]);
        Self {
            directory,
            workspace,
        }
    }
    fn uri(&self, name: &str) -> String {
        file_uri(&self.directory.path().join(name)).unwrap()
    }
    fn write(&self, name: &str, text: &str) {
        fs::write(self.directory.path().join(name), text).unwrap();
    }
}
fn pos(text: &str, needle: &str, nth: usize, delta: usize) -> Position {
    let (byte, _) = text
        .match_indices(needle)
        .nth(nth)
        .unwrap_or_else(|| panic!("Missing {needle} occurrence {nth}"));
    position_at(text, byte + delta)
}
fn labels(result: &Value) -> Vec<&str> {
    result["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["label"].as_str().unwrap())
        .collect()
}

#[test]
fn definitions_and_references_exclude_prose_and_comments() {
    let text =
        "-> hall\n=== hall ===\nThe word hall is prose.\n// -> hall\n* [Go] -> hall\n-> END\n";
    let mut f = Fixture::new(&[("main.ink", text)]);
    let uri = f.uri("main.ink");
    assert_eq!(
        f.workspace.definitions(&uri, pos(text, "hall", 0, 0))[0]
            .range
            .start,
        pos(text, "hall", 1, 0)
    );
    let refs = f.workspace.references(&uri, pos(text, "hall", 1, 0), false);
    assert_eq!(
        refs.iter().map(|l| l.range.start).collect::<Vec<_>>(),
        vec![pos(text, "hall", 0, 0), pos(text, "hall", 4, 0)]
    );
    assert_eq!(
        f.workspace
            .references(&uri, pos(text, "hall", 0, 0), true)
            .len(),
        3
    );
    for (needle, nth) in [("hall", 2), ("hall", 3), ("END", 0)] {
        assert!(
            f.workspace
                .definitions(&uri, pos(text, needle, nth, 0))
                .is_empty()
        );
    }
}
#[test]
fn parameters_and_temps_shadow_only_in_their_own_flow() {
    let text = "VAR x = 1\n=== first(x) ===\n{x}\n=== second(x) ===\n{x}\n=== third ===\n{x}\n~ temp x = 2\n{x}\n";
    let mut f = Fixture::new(&[("main.ink", text)]);
    let uri = f.uri("main.ink");
    for (nth, expected) in [
        pos(text, "first(x)", 0, 6),
        pos(text, "second(x)", 0, 7),
        pos(text, "x", 0, 0),
        pos(text, "temp x", 0, 5),
    ]
    .into_iter()
    .enumerate()
    {
        assert_eq!(
            f.workspace.definitions(&uri, pos(text, "{x}", nth, 1))[0]
                .range
                .start,
            expected
        );
    }
    assert_eq!(
        f.workspace
            .references(&uri, pos(text, "first(x)", 0, 6), false)
            .len(),
        1
    );
}
#[test]
fn stitches_and_labels_resolve_relative_and_qualified_paths() {
    let text = "=== first ===\n= room\n* (again) [Wait] -> again\n-> room\n=== second ===\n= room\n-> room\n-> first.room.again\n";
    let mut f = Fixture::new(&[("main.ink", text)]);
    let uri = f.uri("main.ink");
    for n in 0..2 {
        assert_eq!(
            f.workspace.definitions(&uri, pos(text, "-> room", n, 3))[0]
                .range
                .start,
            pos(text, "= room", n, 2)
        );
    }
    assert_eq!(
        f.workspace
            .definitions(&uri, pos(text, "first.room.again", 0, 6))[0]
            .range
            .start,
        pos(text, "= room", 0, 2)
    );
    assert_eq!(
        f.workspace
            .definitions(&uri, pos(text, "first.room.again", 0, 11))[0]
            .range
            .start,
        pos(text, "(again)", 0, 1)
    );
    assert_eq!(
        f.workspace
            .references(&uri, pos(text, "(again)", 0, 1), false)
            .len(),
        2
    );
}
#[test]
fn include_graph_handles_cycles_isolation_disk_changes_and_open_buffers() {
    let main = "INCLUDE parts/room.ink\n-> room\n";
    let room = "INCLUDE ../main.ink\n=== room ===\n-> room\n";
    let mut f = Fixture::new(&[
        ("main.ink", main),
        ("parts/room.ink", room),
        ("other.ink", "=== room ===\n-> room\n"),
    ]);
    let uri = f.uri("main.ink");
    let target = f.uri("parts/room.ink");
    let at = pos(main, "-> room", 0, 3);
    assert_eq!(f.workspace.definitions(&uri, at)[0].uri, target);
    assert_eq!(
        f.workspace
            .references(&target, pos(room, "=== room", 0, 4), false)
            .len(),
        2
    );
    assert_eq!(
        f.workspace.definitions(&uri, pos(main, "parts/room", 0, 0))[0].uri,
        target
    );
    f.write("parts/room.ink", "=== renamed ===\n");
    assert!(f.workspace.definitions(&uri, at).is_empty());
    f.workspace
        .open_document(target.clone(), room.to_owned(), 1);
    assert_eq!(f.workspace.definitions(&uri, at).len(), 1);
    f.workspace.close_document(&target);
    fs::remove_file(f.directory.path().join("parts/room.ink")).unwrap();
    assert!(f.workspace.definitions(&uri, at).is_empty());
}
#[test]
fn utf16_crlf_and_unsaved_incremental_changes() {
    let text = "VAR day = 1\r\n=== start ===\r\n你好😀{day}\r\n";
    let mut f = Fixture::new(&[("main.ink", text)]);
    let uri = f.uri("main.ink");
    f.workspace.open_document(uri.clone(), text.to_owned(), 1);
    let at = pos(text, "{day}", 0, 1);
    assert_eq!(at.character, 5);
    assert_eq!(
        f.workspace.definitions(&uri, at)[0].range.start,
        pos(text, "day", 0, 0)
    );
    let declaration = pos(text, "day", 0, 0);
    let change = |start: Position| Change {
        range: Some(ink_lsp::index::Range {
            start,
            end: Position {
                character: start.character + 3,
                ..start
            },
        }),
        text: "night".to_owned(),
    };
    f.workspace
        .change_document(&uri, vec![change(declaration), change(at)], 2);
    assert_eq!(
        f.workspace.definitions(&uri, at)[0].range.start,
        declaration
    );
    f.workspace.change_document(
        &uri,
        vec![Change {
            range: None,
            text: "obsolete".into(),
        }],
        1,
    );
    assert_eq!(f.workspace.definitions(&uri, at).len(), 1);
    f.workspace.close_document(&uri);
    assert_eq!(f.workspace.definitions(&uri, at).len(), 1);
    for (byte, c) in text.char_indices() {
        // LSP cannot address the interior of a CRLF line terminator.
        if c == '\n' && text[..byte].ends_with('\r') {
            continue;
        }
        assert_eq!(offset_at(text, position_at(text, byte)), byte);
    }
}
#[test]
fn functions_constants_lists_and_recovery() {
    let text = "CONST limit = 2\nLIST mood = happy, sad\n=== function twice(x) ===\n~ return x * limit\n=== start ===\n~ temp message = \"value {limit}\"\n{twice(limit)} {mood.happy}\n* [unfinished\n=== target ===\n-> target\n";
    let mut f = Fixture::new(&[("main.ink", text)]);
    let uri = f.uri("main.ink");
    assert_eq!(
        f.workspace.definitions(&uri, pos(text, "{twice", 0, 1))[0]
            .range
            .start,
        pos(text, "twice", 0, 0)
    );
    assert_eq!(
        f.workspace.definitions(&uri, pos(text, "mood.happy", 0, 5))[0]
            .range
            .start,
        pos(text, "happy", 0, 0)
    );
    assert_eq!(
        f.workspace
            .references(&uri, pos(text, "limit", 0, 0), false)
            .len(),
        3
    );
    assert_eq!(
        f.workspace
            .definitions(&uri, pos(text, "-> target", 0, 3))
            .len(),
        1
    );
}
#[test]
fn completion_after_bare_arrow_and_partial_targets() {
    let text = "=== start ===\n->\n=== lobby ===\n=== security_room ===\n";
    let mut f = Fixture::new(&[("main.ink", text)]);
    let uri = f.uri("main.ink");
    let result = f.workspace.completion(
        &uri,
        Position {
            line: 1,
            character: 2,
        },
    );
    for name in ["lobby", "security_room", "END", "DONE"] {
        assert!(labels(&result).contains(&name), "{result}");
    }
    let lobby = result["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["label"] == "lobby")
        .unwrap();
    assert_eq!(
        lobby["textEdit"],
        json!({"range":{"start":{"line":1,"character":2},"end":{"line":1,"character":2}},"newText":"lobby"})
    );
    let edited = text.replace("->\n", "-> sec\n");
    f.workspace.open_document(uri.clone(), edited, 1);
    let result = f.workspace.completion(
        &uri,
        Position {
            line: 1,
            character: 6,
        },
    );
    assert_eq!(labels(&result), vec!["security_room"]);
    assert_eq!(
        result["items"][0]["textEdit"]["range"]["start"]["character"],
        3
    );
}
#[test]
fn completion_uses_qualified_and_local_scopes_and_includes() {
    let text = "INCLUDE other.ink\n=== first ===\n= room\n* (again) [Stay]\n-> \n=== second ===\n= room\n-> first.\n";
    let mut f = Fixture::new(&[
        ("main.ink", text),
        ("other.ink", "=== included ===\n"),
        ("unrelated.ink", "=== hidden ===\n"),
    ]);
    let uri = f.uri("main.ink");
    let result = f.workspace.completion(
        &uri,
        Position {
            line: 4,
            character: 3,
        },
    );
    for name in ["room", "again", "included"] {
        assert!(labels(&result).contains(&name), "{result}");
    }
    assert!(!labels(&result).contains(&"hidden"));
    let result = f.workspace.completion(
        &uri,
        Position {
            line: 7,
            character: 9,
        },
    );
    assert!(labels(&result).contains(&"first.room"), "{result}");
    assert!(!labels(&result).contains(&"room"));
}
#[test]
fn expression_completion_and_prose_suppression() {
    let text = "VAR day = 1\n=== start(floor) ===\n~ temp count = 2\n{da}\n~ flo\n";
    let mut f = Fixture::new(&[("main.ink", text)]);
    let uri = f.uri("main.ink");
    assert_eq!(
        labels(&f.workspace.completion(
            &uri,
            Position {
                line: 3,
                character: 3
            }
        )),
        vec!["day"]
    );
    assert_eq!(
        labels(&f.workspace.completion(
            &uri,
            Position {
                line: 4,
                character: 5
            }
        )),
        vec!["floor"]
    );
    for prose in [
        "A day in the lobby",
        "// -> ",
        "/* -> */",
        "/* unfinished -> ",
        "Escaped \\-> ",
        "~ temp s = \"-> \"",
        "# -> ",
    ] {
        let text = format!("=== lobby ===\n{prose}\n");
        let byte = if let Some(at) = text.find("-> ") {
            at + 3
        } else {
            text.len() - 1
        };
        f.workspace.open_document(uri.clone(), text.clone(), 2);
        assert!(
            labels(&f.workspace.completion(&uri, position_at(&text, byte))).is_empty(),
            "{text}"
        );
    }
}
#[test]
fn completion_edits_preserve_unicode_and_replace_the_whole_partial_word() {
    let text = "=== lobby ===\n你好😀 -> lobXYZ\n";
    let mut f = Fixture::new(&[("main.ink", text)]);
    let uri = f.uri("main.ink");
    let result = f.workspace.completion(&uri, pos(text, "lobXYZ", 0, 3));
    assert_eq!(labels(&result), vec!["lobby"]);
    assert_eq!(
        result["items"][0]["textEdit"],
        json!({"range":{"start":{"line":1,"character":8},"end":{"line":1,"character":14}},"newText":"lobby"})
    );
}
#[test]
fn stdio_protocol_runs_the_native_binary() {
    let f = Fixture::new(&[("main.ink", "=== room ===\n-> room\n")]);
    let uri = f.uri("main.ink");
    let mut child = Command::new(env!("CARGO_BIN_EXE_ink-lsp"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let stdout = child.stdout.take().unwrap();
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut reader = BufReader::new(stdout);
        while let Ok(Some(frame)) = read_frame(&mut reader) {
            tx.send(serde_json::from_slice::<Value>(&frame).unwrap())
                .unwrap();
        }
    });
    let mut send = |method: &str, params: Value, id: Option<i32>| {
        let mut message = json!({"jsonrpc":"2.0","method":method,"params":params});
        if let Some(id) = id {
            message["id"] = json!(id);
        }
        let mut frame = Vec::new();
        write_message(&mut frame, &message).unwrap();
        stdin.write_all(&frame[..12]).unwrap();
        stdin.write_all(&frame[12..]).unwrap();
        stdin.flush().unwrap();
    };
    let receive = || rx.recv_timeout(Duration::from_secs(10)).unwrap();
    send(
        "initialize",
        json!({"rootUri":file_uri(f.directory.path())}),
        Some(0),
    );
    let init = receive();
    assert_eq!(init["result"]["capabilities"]["hoverProvider"], true);
    assert!(
        init["result"]["capabilities"]["completionProvider"]["triggerCharacters"]
            .as_array()
            .unwrap()
            .contains(&json!(">"))
    );
    send(
        "textDocument/didOpen",
        json!({"textDocument":{"uri":uri,"version":1,"text":"=== 新房 ===\n你好😀 -> 新房\n"}}),
        None,
    );
    send(
        "textDocument/definition",
        json!({"textDocument":{"uri":uri},"position":{"line":1,"character":9}}),
        Some(1),
    );
    assert_eq!(
        receive()["result"][0]["range"]["start"],
        json!({"line":0,"character":4})
    );
    send(
        "textDocument/references",
        json!({"textDocument":{"uri":uri},"position":{"line":0,"character":4},"context":{"includeDeclaration":false}}),
        Some(2),
    );
    assert_eq!(
        receive()["result"][0]["range"]["start"],
        json!({"line":1,"character":8})
    );
    send(
        "textDocument/didChange",
        json!({"textDocument":{"uri":uri,"version":2},"contentChanges":[{"text":"=== lobby ===\n-> "}]}),
        None,
    );
    send(
        "textDocument/completion",
        json!({"textDocument":{"uri":uri},"position":{"line":1,"character":3}}),
        Some(3),
    );
    assert!(labels(&receive()["result"]).contains(&"lobby"));
    image::DynamicImage::new_rgba8(8, 8)
        .save(f.directory.path().join("preview.png"))
        .unwrap();
    send(
        "textDocument/didChange",
        json!({"textDocument":{"uri":uri,"version":3},"contentChanges":[{"text":"# image: preview.png\n"}]}),
        None,
    );
    send(
        "textDocument/hover",
        json!({"textDocument":{"uri":uri},"position":{"line":0,"character":10}}),
        Some(7),
    );
    assert!(
        receive()["result"]["contents"]["value"]
            .as_str()
            .unwrap()
            .starts_with("![Image preview](data:image/png;base64,")
    );
    send("unsupported", json!({}), Some(4));
    assert_eq!(receive()["error"]["code"], -32601);
    send("textDocument/definition", json!({}), Some(5));
    assert_eq!(receive()["error"]["code"], -32602);
    send("shutdown", json!({}), Some(6));
    assert_eq!(receive()["result"], Value::Null);
    send("exit", json!({}), None);
    drop(stdin);
    assert!(child.wait().unwrap().success());
}

#[test]
fn image_hover_formats_paths_and_errors() {
    use base64::Engine;
    use image::{DynamicImage, ImageFormat};
    let mut f = Fixture::new(&[("story/main.ink", "")]);
    let uri = f.uri("story/main.ink");
    let decode = |result: Value| {
        let md = result["contents"]["value"].as_str().unwrap();
        let data = md
            .strip_prefix("![Image preview](data:image/png;base64,")
            .expect(md)
            .strip_suffix(')')
            .unwrap();
        image::load_from_memory(
            &base64::engine::general_purpose::STANDARD
                .decode(data)
                .unwrap(),
        )
        .unwrap()
        .to_rgba8()
    };
    for (ext, format) in [
        ("png", ImageFormat::Png),
        ("jpg", ImageFormat::Jpeg),
        ("jpeg", ImageFormat::Jpeg),
        ("gif", ImageFormat::Gif),
        ("webp", ImageFormat::WebP),
        ("bmp", ImageFormat::Bmp),
        ("ico", ImageFormat::Ico),
        ("tiff", ImageFormat::Tiff),
    ] {
        let path = format!("../image.{ext}");
        (if ext == "ico" {
            DynamicImage::new_rgba8(32, 16)
        } else {
            DynamicImage::new_rgb8(32, 16)
        })
        .save_with_format(f.directory.path().join(format!("image.{ext}")), format)
        .unwrap();
        let text = format!("# image: {path}\n");
        f.workspace.open_document(uri.clone(), text.clone(), 1);
        let image = decode(f.workspace.hover(&uri, position_at(&text, 9)));
        assert_eq!(image.dimensions(), (32, 16));
    }
    f.write("电梯 image.svg",r##"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="100"><rect width="50" height="100" fill="#ff0000"/></svg>"##);
    for path in [
        "../电梯 image.svg".to_owned(),
        f.directory
            .path()
            .join("电梯 image.svg")
            .to_string_lossy()
            .into_owned(),
        f.uri("电梯 image.svg"),
    ] {
        let text = format!("电梯😀 # image: \"{path}\"\n");
        f.workspace.open_document(uri.clone(), text.clone(), 1);
        let position = position_at(&text, text.find(&path).unwrap());
        let result = f.workspace.hover(&uri, position);
        assert_eq!(result["range"]["start"], json!(position));
        let image = decode(result);
        assert_eq!(image.get_pixel(10, 10).0, [255, 0, 0, 255]);
        assert_eq!(image.get_pixel(image.width() - 1, 10).0, [0, 0, 0, 0]);
        assert!(
            f.workspace
                .hover(
                    &uri,
                    Position {
                        line: 0,
                        character: 0
                    }
                )
                .is_null()
        );
    }
    for text in [
        "// # image: missing.png\n",
        "/* # image: missing.png */\n",
        "image: missing.png\n",
    ] {
        f.workspace.open_document(uri.clone(), text.into(), 1);
        assert!(
            f.workspace
                .hover(&uri, position_at(text, text.find("missing").unwrap()))
                .is_null()
        );
    }
    f.write("bad.png", "broken");
    fs::File::create(f.directory.path().join("huge.png"))
        .unwrap()
        .set_len(21 * 1024 * 1024)
        .unwrap();
    for (name, error) in [
        ("missing.png", "could not be opened"),
        ("bad.png", "unsupported"),
        ("huge.png", "20 MiB"),
    ] {
        let text = format!("# image: ../{name}\n");
        f.workspace.open_document(uri.clone(), text.clone(), 1);
        let result = f.workspace.hover(&uri, position_at(&text, 9));
        assert!(
            result["contents"]["value"]
                .as_str()
                .unwrap()
                .contains(error),
            "{result}"
        );
    }
    for (width, expected) in [(960, 320), (64, 64)] {
        DynamicImage::new_rgb8(width, 32)
            .save(f.directory.path().join("image.png"))
            .unwrap();
        let text = "# image: ../image.png\n";
        f.workspace.open_document(uri.clone(), text.into(), 1);
        assert_eq!(
            decode(f.workspace.hover(&uri, position_at(text, 9))).width(),
            expected
        );
    }
}
