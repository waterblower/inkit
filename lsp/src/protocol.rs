use crate::{
    index::Position,
    workspace::{Change, Workspace},
};
use serde_json::{Value, json};
use std::io::{self, BufRead, Write};

pub struct RpcError(pub i32, pub String);
#[derive(Default)]
pub struct Server {
    pub workspace: Workspace,
    pub shutdown: bool,
}
fn invalid(message: &str) -> RpcError {
    RpcError(-32602, message.to_owned())
}
fn string<'a>(value: &'a Value, key: &str) -> Result<&'a str, RpcError> {
    value[key].as_str().ok_or_else(|| invalid(key))
}
fn position(params: &Value) -> Result<Position, RpcError> {
    serde_json::from_value(params["position"].clone()).map_err(|_| invalid("position"))
}
fn version(params: &Value) -> Result<i64, RpcError> {
    params["version"].as_i64().ok_or_else(|| invalid("version"))
}
impl Server {
    pub fn dispatch(&mut self, method: &str, params: &Value) -> Result<Value, RpcError> {
        match method {
            "initialize" => {
                let roots = params["workspaceFolders"]
                    .as_array()
                    .map(|folders| {
                        folders
                            .iter()
                            .filter_map(|f| f["uri"].as_str().map(str::to_owned))
                            .collect()
                    })
                    .unwrap_or_else(|| {
                        params["rootUri"]
                            .as_str()
                            .map(str::to_owned)
                            .into_iter()
                            .collect()
                    });
                self.workspace.set_roots(roots);
                Ok(
                    json!({"capabilities": {"positionEncoding": "utf-16", "definitionProvider": true,
                    "referencesProvider": true, "hoverProvider": true,
                    "completionProvider": {"resolveProvider": false, "triggerCharacters": [">", "-", ".", " ", "{", "~"]},
                    "textDocumentSync": {"openClose": true, "change": 2, "save": true},
                    "workspace": {"workspaceFolders": {"supported": true, "changeNotifications": true}}},
                    "serverInfo": {"name": "Ink Language Server", "version": env!("CARGO_PKG_VERSION")}}),
                )
            }
            "textDocument/didOpen" => {
                let doc = &params["textDocument"];
                self.workspace.open_document(
                    string(doc, "uri")?.to_owned(),
                    string(doc, "text")?.to_owned(),
                    version(doc)?,
                );
                Ok(Value::Null)
            }
            "textDocument/didChange" => {
                let doc = &params["textDocument"];
                let changes: Vec<Change> = serde_json::from_value(params["contentChanges"].clone())
                    .map_err(|_| invalid("contentChanges"))?;
                self.workspace
                    .change_document(string(doc, "uri")?, changes, version(doc)?);
                Ok(Value::Null)
            }
            "textDocument/didClose" => {
                self.workspace
                    .close_document(string(&params["textDocument"], "uri")?);
                Ok(Value::Null)
            }
            "workspace/didChangeWorkspaceFolders" => {
                let mut roots = self.workspace.roots.clone();
                if let Some(removed) = params["event"]["removed"].as_array() {
                    roots.retain(|uri| !removed.iter().any(|f| f["uri"].as_str() == Some(uri)));
                }
                if let Some(added) = params["event"]["added"].as_array() {
                    roots.extend(
                        added
                            .iter()
                            .filter_map(|f| f["uri"].as_str().map(str::to_owned)),
                    );
                }
                self.workspace.set_roots(roots);
                Ok(Value::Null)
            }
            "textDocument/definition" => Ok(json!(
                self.workspace
                    .definitions(string(&params["textDocument"], "uri")?, position(params)?)
            )),
            "textDocument/references" => Ok(json!(
                self.workspace.references(
                    string(&params["textDocument"], "uri")?,
                    position(params)?,
                    params["context"]["includeDeclaration"]
                        .as_bool()
                        .unwrap_or(false)
                )
            )),
            "textDocument/hover" => Ok(self
                .workspace
                .hover(string(&params["textDocument"], "uri")?, position(params)?)),
            "textDocument/completion" => Ok(self
                .workspace
                .completion(string(&params["textDocument"], "uri")?, position(params)?)),
            "shutdown" => {
                self.shutdown = true;
                Ok(Value::Null)
            }
            "initialized"
            | "$/cancelRequest"
            | "$/setTrace"
            | "textDocument/didSave"
            | "workspace/didChangeWatchedFiles"
            | "workspace/didChangeConfiguration" => Ok(Value::Null),
            _ => Err(RpcError(-32601, format!("Unsupported method: {method}"))),
        }
    }
}

pub fn read_frame(reader: &mut impl BufRead) -> io::Result<Option<Vec<u8>>> {
    let mut length = None;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line)? == 0 {
            return if length.is_none() {
                Ok(None)
            } else {
                Err(io::ErrorKind::UnexpectedEof.into())
            };
        }
        if line == "\r\n" || line == "\n" {
            break;
        }
        if let Some((name, value)) = line.split_once(':')
            && name.eq_ignore_ascii_case("Content-Length")
        {
            length = Some(value.trim().parse::<usize>().map_err(|_| {
                io::Error::new(io::ErrorKind::InvalidData, "Invalid Content-Length")
            })?);
        }
    }
    let length = length
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "Missing Content-Length"))?;
    if length > 64 * 1024 * 1024 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "LSP frame exceeds 64 MiB",
        ));
    }
    let mut body = vec![0; length];
    reader.read_exact(&mut body)?;
    Ok(Some(body))
}
pub fn write_message(writer: &mut impl Write, message: &Value) -> io::Result<()> {
    let body = serde_json::to_vec(message)?;
    write!(writer, "Content-Length: {}\r\n\r\n", body.len())?;
    writer.write_all(&body)?;
    writer.flush()
}
pub fn run(reader: &mut impl BufRead, writer: &mut impl Write) -> io::Result<i32> {
    let mut server = Server::default();
    while let Some(body) = read_frame(reader)? {
        let message: Value = match serde_json::from_slice(&body) {
            Ok(value) => value,
            Err(_) => {
                write_message(
                    writer,
                    &json!({"jsonrpc":"2.0","id":null,"error":{"code":-32700,"message":"Invalid JSON"}}),
                )?;
                continue;
            }
        };
        let Some(method) = message["method"].as_str() else {
            continue;
        };
        if method == "exit" {
            return Ok(if server.shutdown { 0 } else { 1 });
        }
        let result = server.dispatch(method, &message["params"]);
        if let Some(id) = message.get("id") {
            let response = match result {
                Ok(result) => json!({"jsonrpc":"2.0","id":id,"result":result}),
                Err(RpcError(code, reason)) => {
                    json!({"jsonrpc":"2.0","id":id,"error":{"code":code,"message":reason}})
                }
            };
            write_message(writer, &response)?;
        } else if let Err(RpcError(code, reason)) = result
            && code != -32601
        {
            eprintln!("Ink LSP: {method}: {reason}");
        }
    }
    Ok(if server.shutdown { 0 } else { 1 })
}
