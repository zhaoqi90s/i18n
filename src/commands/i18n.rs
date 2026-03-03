use zed_extension_api::{serde_json, SlashCommandOutput, SlashCommandOutputSection, Worktree};

use crate::config::load_config;
use crate::translation::{find_translation, format_value, resolve_translation};

/// `/i18n <key> [lang]` — look up a single translation key and display its value.
pub fn run_i18n(
    args: Vec<String>,
    worktree: Option<&Worktree>,
) -> Result<SlashCommandOutput, String> {
    // --- validate args ---
    let key = args.first().ok_or_else(|| {
        concat!(
            "Usage: /i18n <key> [lang]\n\n",
            "Examples:\n",
            "  /i18n common.button.save\n",
            "  /i18n common.button.save zh\n",
            "  /i18n auth.errors.invalidEmail fr\n\n",
            "The default language is 'en' (override with .i18n-viewer.json)."
        )
        .to_string()
    })?;

    let worktree = worktree
        .ok_or_else(|| "No open workspace. Please open a project folder first.".to_string())?;

    let config = load_config(worktree);
    let lang = args
        .get(1)
        .map(String::as_str)
        .unwrap_or(config.default_lang.as_str())
        .to_string();

    // --- find translation file (cache-first) ---
    let (content, found_path) = find_translation(worktree, &lang, &config).map_err(|base_err| {
        // Read .i18n-viewer.json to decide which hint to append.
        let cfg_raw = worktree
            .read_text_file(".i18n-viewer.json")
            .unwrap_or_default();
        let cfg_json: serde_json::Value = serde_json::from_str(&cfg_raw).unwrap_or_default();

        // Does remoteSources have an entry for this exact language?
        let remote_url = cfg_json
            .get("remoteSources")
            .and_then(|rs| rs.as_object())
            .and_then(|m| m.get(&lang))
            .and_then(|v| v.as_str())
            .map(String::from);

        match remote_url {
            Some(url) => format!(
                "{base_err}\n\n\
                 Hint: remoteSources[\"{lang}\"] is configured ({url})\n\
                 but the local cache is empty.\n\
                 → Run /i18n-sync to download and cache it, then retry."
            ),
            None => format!(
                "{base_err}\n\n\
                 Tip: add the language file path or a remote source in \
                 .i18n-viewer.json:\n\
                 {{\n  \
                 \"defaultLang\": \"{lang}\",\n  \
                 \"localPaths\": [\"locales/{{lang}}.json\"],\n  \
                 \"remoteSources\": {{ \"{lang}\": \"https://example.com/{lang}.json\" }}\n\
                 }}"
            ),
        }
    })?;

    // --- parse JSON ---
    let json: serde_json::Value = serde_json::from_str(&content)
        .map_err(|e| format!("Failed to parse '{found_path}': {e}"))?;

    // --- look up key (prefix-aware) ---
    let key_prefix = config.key_prefix.as_str();
    let full_key = if key_prefix.is_empty() {
        key.to_string()
    } else {
        format!("{key_prefix}{key}")
    };

    let value = resolve_translation(&json, key, key_prefix)
        .ok_or_else(|| format!("Key '{full_key}' not found in '{found_path}'"))?;

    let translation = format_value(value);
    let is_namespace = matches!(
        value,
        serde_json::Value::Object(_) | serde_json::Value::Array(_)
    );

    // --- build output ---
    let kind_hint = if is_namespace {
        " (namespace — showing all leaf keys)"
    } else {
        ""
    };

    let header = format!("i18n: {full_key} [{lang}]{kind_hint}");
    let rule = "─".repeat(header.len().min(72));
    let source_line = format!("(from {found_path})");
    let text = format!("{header}\n{rule}\n{translation}\n\n{source_line}");
    let label = format!("i18n: {full_key} [{lang}]");

    Ok(SlashCommandOutput {
        sections: vec![SlashCommandOutputSection {
            range: (0..text.len()).into(),
            label,
        }],
        text,
    })
}
