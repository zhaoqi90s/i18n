use zed_extension_api::{serde_json, SlashCommandOutput, SlashCommandOutputSection, Worktree};

use crate::config::load_config;
use crate::translation::{collect_entries, find_translation, fuzzy_score};

/// Maximum number of results to display (prevents overwhelming output).
const MAX_RESULTS: usize = 50;

/// `/i18n-search <text> [lang]` — fuzzy-search translation values and return matching keys.
///
/// Matching tiers (highest score wins):
///   1. Exact case-insensitive match
///   2. Substring match (position & coverage weighted)
///   3. All query words appear in the value (any order)
///   4. All query characters appear in order (classic fuzzy)
pub fn run_i18n_search(
    args: Vec<String>,
    worktree: Option<&Worktree>,
) -> Result<SlashCommandOutput, String> {
    // --- validate args ---
    let query = args.first().ok_or_else(|| {
        concat!(
            "Usage: /i18n-search <text> [lang]\n\n",
            "Examples:\n",
            "  /i18n-search 保存\n",
            "  /i18n-search save\n",
            "  /i18n-search cancel en\n",
            "  /i18n-search 取消 zh\n\n",
            "Fuzzy-searches translation values and returns the matching keys.\n",
            "The default language is 'en' (override with .i18n-viewer.json).",
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
    let (content, found_path) = find_translation(worktree, &lang, &config)?;

    // --- parse JSON ---
    let json: serde_json::Value = serde_json::from_str(&content)
        .map_err(|e| format!("Failed to parse '{found_path}': {e}"))?;

    // --- collect all leaf (key, value) pairs ---
    let mut entries: Vec<(String, String)> = Vec::new();
    collect_entries(&json, "", &mut entries);

    // --- fuzzy match & score ---
    let mut matches: Vec<(u32, String, String)> = entries
        .into_iter()
        .filter_map(|(key, value)| fuzzy_score(query, &value).map(|score| (score, key, value)))
        .collect();

    // Sort by score descending, then by key alphabetically for stable output
    matches.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));

    // --- build output ---
    let header = format!("i18n search: \"{query}\" [{lang}] — {found_path}");
    let rule = "─".repeat(header.len().min(72));

    if matches.is_empty() {
        let text = format!(
            "{header}\n{rule}\n\nNo matches found for \"{query}\".\n\n\
             Tip: try a shorter or simpler query for broader fuzzy matching."
        );
        let label = format!("i18n search: \"{query}\" [{lang}] (0 matches)");
        return Ok(SlashCommandOutput {
            sections: vec![SlashCommandOutputSection {
                range: (0..text.len()).into(),
                label,
            }],
            text,
        });
    }

    let total = matches.len();
    let displayed = matches.len().min(MAX_RESULTS);

    let result_lines: Vec<String> = matches
        .iter()
        .take(MAX_RESULTS)
        .map(|(_, key, value)| format!("{key}\n  → {value}"))
        .collect();

    let result_text = result_lines.join("\n");

    let footer = if total > MAX_RESULTS {
        format!("({displayed} of {total} matches shown — refine your query to narrow results)")
    } else {
        format!("({total} match{})", if total == 1 { "" } else { "es" })
    };

    let text = format!("{header}\n{rule}\n\n{result_text}\n\n{footer}");
    let label = format!("i18n search: \"{query}\" [{lang}] ({total} matches)");

    Ok(SlashCommandOutput {
        sections: vec![SlashCommandOutputSection {
            range: (0..text.len()).into(),
            label,
        }],
        text,
    })
}
