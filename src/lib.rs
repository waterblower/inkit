use std::fs;
use zed_extension_api::{self as zed, Command, LanguageServerId, Worktree};

const REPOSITORY: &str = "waterblower/inkit";
const SERVER_RELEASE: &str = "main-12f0519270f8f29c0b23f2963ac582c871409c6a";
struct InkExtension;

impl zed::Extension for InkExtension {
    fn new() -> Self {
        Self
    }
    fn language_server_command(
        &mut self,
        language_server_id: &LanguageServerId,
        worktree: &Worktree,
    ) -> zed::Result<Command> {
        if let Some(binary) =
            zed::settings::LspSettings::for_worktree(language_server_id.as_ref(), worktree)?.binary
            && let Some(path) = binary.path
        {
            return Ok(Command {
                command: path,
                args: binary.arguments.unwrap_or_default(),
                env: binary.env.unwrap_or_default().into_iter().collect(),
            });
        }
        let local_server = env!("INKIT_DEV_SERVER");
        if !local_server.is_empty() {
            return Ok(Command {
                command: local_server.to_owned(),
                args: Vec::new(),
                env: Default::default(),
            });
        }
        if let Some(path) = worktree.which("ink-lsp") {
            return Ok(Command {
                command: path,
                args: Vec::new(),
                env: Default::default(),
            });
        }
        let (os, arch) = zed::current_platform();
        let target = match (os, arch) {
            (zed::Os::Mac, zed::Architecture::Aarch64) => "aarch64-apple-darwin",
            (zed::Os::Mac, zed::Architecture::X8664) => "x86_64-apple-darwin",
            (zed::Os::Linux, zed::Architecture::Aarch64) => "aarch64-unknown-linux-gnu",
            (zed::Os::Linux, zed::Architecture::X8664) => "x86_64-unknown-linux-gnu",
            (zed::Os::Windows, zed::Architecture::X8664) => "x86_64-pc-windows-msvc",
            _ => return Err("No prebuilt Ink server for this platform. Install ink-lsp on PATH or configure lsp.ink-navigation.binary.path.".into()),
        };
        let version = SERVER_RELEASE;
        let directory = format!("ink-lsp-{version}-{target}");
        let filename = if matches!(os, zed::Os::Windows) {
            "ink-lsp.exe"
        } else {
            "ink-lsp"
        };
        let executable = format!("{directory}/{filename}");
        if !fs::metadata(&executable).is_ok_and(|metadata| metadata.is_file()) {
            zed::set_language_server_installation_status(
                language_server_id,
                &zed::LanguageServerInstallationStatus::Downloading,
            );
            let asset_name = format!("ink-lsp-{target}.tar.gz");
            let download_url =
                format!("https://github.com/{REPOSITORY}/releases/download/{version}/{asset_name}");
            let staging = format!("{directory}.download");
            zed::download_file(&download_url, &staging, zed::DownloadedFileType::GzipTar)?;
            zed::make_file_executable(&format!("{staging}/{filename}"))?;
            fs::rename(&staging, &directory).map_err(|error| error.to_string())?;
        }
        zed::make_file_executable(&executable)?;
        Ok(Command {
            command: executable,
            args: Vec::new(),
            env: Default::default(),
        })
    }
}
zed::register_extension!(InkExtension);
