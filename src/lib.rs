mod commands;
mod config;
mod langs;
mod translation;

use commands::{run_i18n, run_i18n_keys, run_i18n_sync};
use langs::COMMON_LANGS;
use zed_extension_api::{
    self as zed, node_binary_path, Command, LanguageServerId, SlashCommand,
    SlashCommandArgumentCompletion, SlashCommandOutput, Worktree,
};

// ---------------------------------------------------------------------------
// Embedded LSP server
// ---------------------------------------------------------------------------

/// The Node.js LSP server script, embedded at compile time.
/// On first use it is written into the extension's `work/lsp/` directory so
/// that Node.js can load it regardless of how Zed resolves the extension path.
const SERVER_JS: &str = include_str!("../lsp/server.js");

// ---------------------------------------------------------------------------
// Extension entry point
// ---------------------------------------------------------------------------

struct I18nTranslatorExtension {
    /// Cached absolute path to the `server.js` written into the extension's
    /// work directory. Resolved (and the file written) once on the first
    /// `language_server_command` call.
    server_path: Option<String>,
}

impl zed::Extension for I18nTranslatorExtension {
    fn new() -> Self {
        I18nTranslatorExtension { server_path: None }
    }

    // ── Language server (hover preview) ──────────────────────────────────────

    fn language_server_command(
        &mut self,
        _language_server_id: &LanguageServerId,
        _worktree: &Worktree,
    ) -> zed::Result<Command> {
        // Zed sets the WASM extension's CWD to the `work/<id>/` directory.
        // We embed server.js at compile time and write it there so Node.js can
        // always find it, regardless of whether this is a dev or installed extension.
        let server_path = match &self.server_path {
            Some(p) => p.clone(),
            None => {
                let cwd = std::env::current_dir()
                    .map_err(|e| format!("Could not determine extension work directory: {e}"))?;

                let lsp_dir = cwd.join("lsp");
                std::fs::create_dir_all(&lsp_dir)
                    .map_err(|e| format!("Could not create lsp/ directory: {e}"))?;

                let server_js_path = lsp_dir.join("server.js");
                std::fs::write(&server_js_path, SERVER_JS)
                    .map_err(|e| format!("Could not write server.js: {e}"))?;

                let p = server_js_path.to_string_lossy().into_owned();
                self.server_path = Some(p.clone());
                p
            }
        };

        let node = node_binary_path().map_err(|e| format!("Could not find Node.js binary: {e}"))?;

        Ok(Command {
            command: node,
            args: vec![server_path],
            env: vec![],
        })
    }

    // ── Slash command argument completions ────────────────────────────────────

    fn complete_slash_command_argument(
        &self,
        command: SlashCommand,
        args: Vec<String>,
    ) -> Result<Vec<SlashCommandArgumentCompletion>, String> {
        let lang_completions = || {
            COMMON_LANGS
                .iter()
                .map(|(code, name)| SlashCommandArgumentCompletion {
                    label: format!("{code} — {name}"),
                    new_text: code.to_string(),
                    run_command: true,
                })
                .collect::<Vec<_>>()
        };

        match command.name.as_str() {
            // /i18n <key> [lang] — complete the language once the key is typed
            "i18n" => {
                if args.len() >= 1 {
                    Ok(lang_completions())
                } else {
                    Ok(vec![])
                }
            }
            // /i18n-keys [lang] and /i18n-sync [lang] — only arg is the language
            "i18n-keys" | "i18n-sync" => Ok(lang_completions()),

            unknown => Err(format!("unknown slash command: \"{unknown}\"")),
        }
    }

    // ── Slash command execution ───────────────────────────────────────────────

    fn run_slash_command(
        &self,
        command: SlashCommand,
        args: Vec<String>,
        worktree: Option<&Worktree>,
    ) -> Result<SlashCommandOutput, String> {
        match command.name.as_str() {
            "i18n" => run_i18n(args, worktree),
            "i18n-keys" => run_i18n_keys(args, worktree),
            "i18n-sync" => {
                // Derive the IPC file path from the server.js location stored on self.
                // server.js lives at  <work_dir>/lsp/server.js
                // .sync-request goes at  <work_dir>/.sync-request
                let sync_path = self.server_path.as_deref().and_then(|p| {
                    std::path::Path::new(p)
                        .parent() // <work_dir>/lsp
                        .and_then(|p| p.parent()) // <work_dir>
                        .map(|d| d.join(".sync-request"))
                });
                run_i18n_sync(args, worktree, sync_path.as_deref())
            }
            unknown => Err(format!("unknown slash command: \"{unknown}\"")),
        }
    }
}

zed::register_extension!(I18nTranslatorExtension);
