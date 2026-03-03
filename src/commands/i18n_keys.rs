use zed_extension_api::{serde_json, SlashCommandOutput, SlashCommandOutputSection, Worktree};

use crate::config::load_config;
use crate::translation::{collect_keys, find_translation};

/// `/i18n-keys [lang]` — list all leaf keys in the translation file, sorted alphabetically.
pub fn run_i18n_keys(
    args: Vec<String>,
    worktree: Option<&Worktree>,
) -> Result<SlashCommandOutput, String> {
    let worktree = worktree
        .ok_or_else(|| "No open workspace. Please open a project folder first.".to_string())?;

    let config = load_config(worktree);
    let lang = args
        .first()
        .map(String::as_str)
        .unwrap_or(config.default_lang.as_str())
        .to_string();

    // --- find translation file (cache-first) ---
    let (content, found_path) = find_translation(worktree, &lang, &config)?;

    // --- parse JSON ---
    let json: serde_json::Value = serde_json::from_str(&content)
        .map_err(|e| format!("Failed to parse '{found_path}': {e}"))?;

    // --- collect all leaf keys ---
    let mut keys: Vec<String> = Vec::new();
    collect_keys(&json, "", &mut keys);
    keys.sort_unstable();

    let total = keys.len();
    let key_list = keys.join("\n");

    // --- build output ---
    let header = format!("i18n keys [{lang}] — {found_path}");
    let rule = "─".repeat(header.len().min(72));
    let footer = format!("({total} keys total)");
    let text = format!("{header}\n{rule}\n{key_list}\n\n{footer}");
    let label = format!("i18n keys [{lang}] ({total} keys)");

    Ok(SlashCommandOutput {
        sections: vec![SlashCommandOutputSection {
            range: (0..text.len()).into(),
            label,
        }],
        text,
    })
}
