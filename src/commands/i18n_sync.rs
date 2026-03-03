use zed_extension_api::{serde_json, SlashCommandOutput, SlashCommandOutputSection, Worktree};

use crate::config::load_config;
use crate::translation::{collect_keys, resolve_paths};

/// `/i18n-sync [lang]` — show cache status and trigger a remote sync via the LSP server.
///
/// Because slash commands run inside Zed's WASM sandbox they cannot make HTTP
/// requests or write directly to the workspace.  Instead this function writes
/// a `.sync-request` JSON file into the extension work directory; the LSP
/// server (`server.js`) polls for that file every 500 ms and performs the actual
/// HTTP fetch + disk-cache write in the Node.js process.
pub fn run_i18n_sync(
    args: Vec<String>,
    worktree: Option<&Worktree>,
    // Absolute path to `.sync-request` derived from the server.js location.
    // `None` when the language server hasn't started yet (no supported file open).
    sync_request_path: Option<&std::path::Path>,
) -> Result<SlashCommandOutput, String> {
    let worktree = worktree
        .ok_or_else(|| "No open workspace. Please open a project folder first.".to_string())?;

    let config = load_config(worktree);

    // ── Parse full config for remoteSources / languages ───────────────────────
    let cfg_raw = worktree
        .read_text_file(".i18n-viewer.json")
        .unwrap_or_default();
    let cfg_json: serde_json::Value =
        serde_json::from_str(&cfg_raw).unwrap_or(serde_json::Value::Object(Default::default()));

    // remoteSources: { "en": "https://...", "zh": "https://..." }
    let mut remote_sources: Vec<(String, String)> = cfg_json
        .get("remoteSources")
        .and_then(|rs| rs.as_object())
        .map(|map| {
            map.iter()
                .filter_map(|(k, v)| v.as_str().map(|url| (k.clone(), url.to_string())))
                .collect()
        })
        .unwrap_or_default();
    remote_sources.sort_by(|a, b| a.0.cmp(&b.0));

    // Build the full ordered list of languages to report on:
    //   defaultLang → languages[] → any extra keys in remoteSources
    let mut all_langs: Vec<String> = vec![config.default_lang.clone()];
    if let Some(arr) = cfg_json.get("languages").and_then(|l| l.as_array()) {
        for v in arr {
            if let Some(s) = v.as_str() {
                let s = s.to_string();
                if !all_langs.contains(&s) {
                    all_langs.push(s);
                }
            }
        }
    }
    for (lang, _) in &remote_sources {
        if !all_langs.contains(lang) {
            all_langs.push(lang.clone());
        }
    }

    // If caller passed a specific lang, narrow to just that one.
    let target = args.first().cloned();
    let langs_to_check: Vec<String> = match &target {
        Some(lang) => vec![lang.clone()],
        None => all_langs.clone(),
    };

    // ── Build the report ──────────────────────────────────────────────────────
    let mut lines: Vec<String> = Vec::new();
    let cache_dir = &config.cache_dir;

    // Header
    match &target {
        Some(lang) => lines.push(format!("i18n Sync Status — [{lang}]")),
        None => lines.push("i18n Sync Status".to_string()),
    }
    lines.push("─".repeat(48));
    lines.push(String::new());

    // Remote-sources section
    if remote_sources.is_empty() {
        lines.push("Remote sources: none configured".to_string());
        lines.push(
            "  Tip: add \"remoteSources\" to .i18n-viewer.json to enable remote fetching."
                .to_string(),
        );
    } else {
        lines.push(format!(
            "Remote sources ({} configured):",
            remote_sources.len()
        ));
        for (lang, url) in &remote_sources {
            lines.push(format!("  [{lang}]  {url}"));
        }
    }
    lines.push(String::new());

    // Cache-status section
    lines.push(format!("Cache status  ({cache_dir}/:)"));
    lines.push("─".repeat(32));

    let mut needs_sync: Vec<String> = Vec::new();

    for lang in &langs_to_check {
        let cache_path = format!("{cache_dir}/{lang}.json");
        let has_remote = remote_sources.iter().any(|(l, _)| l == lang);

        match worktree.read_text_file(&cache_path) {
            Ok(content) => {
                let key_count = serde_json::from_str::<serde_json::Value>(&content)
                    .ok()
                    .map(|json| {
                        let mut keys = Vec::new();
                        collect_keys(&json, "", &mut keys);
                        keys.len()
                    })
                    .unwrap_or(0);

                let remote_hint = if has_remote { "" } else { "  (local only)" };
                lines.push(format!(
                    "  [{lang}]  ✓  {cache_path}  ({key_count} keys){remote_hint}"
                ));
            }
            Err(_) => {
                if has_remote {
                    lines.push(format!(
                        "  [{lang}]  ✗  not cached  →  will fetch from remote on next hover"
                    ));
                    needs_sync.push(lang.clone());
                } else {
                    // Check whether a local source file exists instead.
                    let local_found = resolve_paths(lang, &config.local_paths)
                        .iter()
                        .any(|p| worktree.read_text_file(p).is_ok());
                    if local_found {
                        lines.push(format!(
                            "  [{lang}]  ✓  local file found  (no remote source)"
                        ));
                    } else {
                        lines.push(format!("  [{lang}]  ✗  no cache and no local file found"));
                        needs_sync.push(lang.clone());
                    }
                }
            }
        }
    }

    lines.push(String::new());

    // ── Trigger sync via IPC file ─────────────────────────────────────────────
    if remote_sources.is_empty() {
        lines.push(
            "Nothing to sync — no remote sources are configured in .i18n-viewer.json.".to_string(),
        );
    } else {
        let langs_to_sync: Vec<&str> = match &target {
            Some(lang) => vec![lang.as_str()],
            None => remote_sources.iter().map(|(l, _)| l.as_str()).collect(),
        };

        let workspace_root = worktree.root_path();
        let payload = serde_json::json!({
            "workspace": workspace_root,
            "langs": langs_to_sync,
        });

        match sync_request_path {
            Some(req_path) => match std::fs::write(req_path, payload.to_string()) {
                Ok(_) => {
                    lines.push(format!(
                        "⟳  Sync triggered for: {}",
                        langs_to_sync.join(", ")
                    ));
                    lines.push(
                        "   The LSP server will download and cache the packs within a second."
                            .to_string(),
                    );
                }
                Err(e) => {
                    lines.push(format!("✗  Could not write sync request: {e}"));
                    lines.push(
                        "   Try hovering over any i18n key to trigger a lazy re-fetch.".to_string(),
                    );
                }
            },
            None => {
                lines.push(String::new());
                lines.push(
                    "⚠  The i18n LSP server has not started yet (open a supported source file first).".to_string(),
                );
                lines.push(
                    "   Once the server is running, re-run /i18n-sync to trigger a download."
                        .to_string(),
                );
            }
        }
    }

    // Suppress unused-variable warning: needs_sync is computed for potential
    // future use (e.g. auto-triggering); the list is intentionally not printed
    // because the per-language lines already communicate the same information.
    let _ = needs_sync;

    let text = lines.join("\n");
    let label = match target {
        Some(ref lang) => format!("i18n sync [{lang}]"),
        None => "i18n sync status".to_string(),
    };

    Ok(SlashCommandOutput {
        sections: vec![SlashCommandOutputSection {
            range: (0..text.len()).into(),
            label,
        }],
        text,
    })
}
