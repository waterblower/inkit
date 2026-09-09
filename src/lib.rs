use std::fs;
use zed_extension_api::{self as zed, Command, LanguageServerId, Worktree};

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
        // Allow a separately built binary (e.g. on a remote host) to be configured.
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
        let host = include_str!("../.local/native/host.txt").trim();
        let (os, arch) = zed::current_platform();
        let os_matches = match os {
            zed::Os::Mac => host.contains("apple-darwin"),
            zed::Os::Linux => host.contains("linux"),
            zed::Os::Windows => host.contains("windows"),
        };
        let arch_matches = match arch {
            zed::Architecture::Aarch64 => host.starts_with("aarch64-"),
            zed::Architecture::X8664 => host.starts_with("x86_64-"),
            zed::Architecture::X86 => host.starts_with("i686-") || host.starts_with("i586-"),
        };
        if !os_matches || !arch_matches {
            return Err(format!(
                "This Ink dev extension bundles a server for {host}. Rebuild on this host or configure lsp.ink-navigation.binary.path."
            ));
        }
        let contents = include_bytes!("../.local/native/ink-lsp");
        // A content fingerprint avoids overwriting a running executable on
        // platforms that lock it. New builds get a distinct executable path.
        let fingerprint = contents.iter().fold(0xcbf29ce484222325_u64, |hash, byte| {
            (hash ^ *byte as u64).wrapping_mul(0x100000001b3)
        });
        let directory = std::env::current_dir()
            .map_err(|err| err.to_string())?
            .join("ink-native");
        fs::create_dir_all(&directory).map_err(|err| err.to_string())?;
        let suffix = if matches!(os, zed::Os::Windows) {
            ".exe"
        } else {
            ""
        };
        let executable = directory.join(format!("ink-lsp-{fingerprint:016x}{suffix}"));
        if !executable.exists() {
            fs::write(&executable, contents).map_err(|err| err.to_string())?;
        }
        let command = executable.to_string_lossy().into_owned();
        zed::make_file_executable(&command)?;
        Ok(Command {
            command,
            args: Vec::new(),
            env: Default::default(),
        })
    }
}

zed::register_extension!(InkExtension);
