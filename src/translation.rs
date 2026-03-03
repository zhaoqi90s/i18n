use zed_extension_api::{serde_json, Worktree};

use crate::config::Config;

// ---------------------------------------------------------------------------
// Path resolution
// ---------------------------------------------------------------------------

/// Replace `{lang}` in each template with the actual language code.
pub fn resolve_paths(lang: &str, templates: &[String]) -> Vec<String> {
    templates
        .iter()
        .map(|t| t.replace("{lang}", lang))
        .collect()
}

/// Try each candidate path in order; return the first readable content plus
/// which path succeeded. If none succeed, return all tried paths as an error.
pub fn find_translation_file(
    worktree: &Worktree,
    candidates: &[String],
) -> Result<(String, String), Vec<String>> {
    let mut tried = Vec::new();
    for path in candidates {
        match worktree.read_text_file(path) {
            Ok(content) => return Ok((content, path.clone())),
            Err(_) => tried.push(path.clone()),
        }
    }
    Err(tried)
}

/// Resolve a translation file for `lang` using the priority order:
///   1. `cacheDir/<lang>.json`  — disk cache written by the LSP server
///   2. `localPaths` templates  — source files inside the project
///
/// Returns `(json_content, source_path)` or a human-readable error string
/// listing every path that was attempted.
pub fn find_translation(
    worktree: &Worktree,
    lang: &str,
    config: &Config,
) -> Result<(String, String), String> {
    let mut tried: Vec<String> = Vec::new();

    // 1. Cache directory (populated by the LSP server after a remote fetch).
    let cache_path = format!("{}/{}.json", config.cache_dir, lang);
    match worktree.read_text_file(&cache_path) {
        Ok(content) => return Ok((content, cache_path)),
        Err(_) => tried.push(cache_path),
    }

    // 2. Local source files.
    let local_candidates = resolve_paths(lang, &config.local_paths);
    match find_translation_file(worktree, &local_candidates) {
        Ok(result) => return Ok(result),
        Err(local_tried) => tried.extend(local_tried),
    }

    let list = tried
        .iter()
        .map(|p| format!("  • {p}"))
        .collect::<Vec<_>>()
        .join("\n");

    Err(format!(
        "No translation file found for language '{lang}'.\n\nSearched:\n{list}",
    ))
}

// ---------------------------------------------------------------------------
// JSON value formatting
// ---------------------------------------------------------------------------

/// Recursively walk a JSON value and produce flat `key: value` lines.
/// `prefix` is the dot-path accumulated so far (empty for the root call).
pub fn collect_lines(value: &serde_json::Value, prefix: &str, lines: &mut Vec<String>) {
    match value {
        serde_json::Value::Object(map) => {
            for (k, v) in map {
                let full_key = if prefix.is_empty() {
                    k.clone()
                } else {
                    format!("{prefix}.{k}")
                };
                collect_lines(v, &full_key, lines);
            }
        }
        serde_json::Value::Array(arr) => {
            for (i, v) in arr.iter().enumerate() {
                let indexed_key = if prefix.is_empty() {
                    format!("[{i}]")
                } else {
                    format!("{prefix}[{i}]")
                };
                collect_lines(v, &indexed_key, lines);
            }
        }
        serde_json::Value::String(s) => {
            if prefix.is_empty() {
                lines.push(s.clone());
            } else {
                lines.push(format!("{prefix}: {s}"));
            }
        }
        serde_json::Value::Number(n) => {
            if prefix.is_empty() {
                lines.push(n.to_string());
            } else {
                lines.push(format!("{prefix}: {n}"));
            }
        }
        serde_json::Value::Bool(b) => {
            if prefix.is_empty() {
                lines.push(b.to_string());
            } else {
                lines.push(format!("{prefix}: {b}"));
            }
        }
        serde_json::Value::Null => {
            if prefix.is_empty() {
                lines.push("(null)".to_string());
            } else {
                lines.push(format!("{prefix}: (null)"));
            }
        }
    }
}

/// Format any JSON value as human-readable text.
/// - Scalars are returned as-is.
/// - Objects / arrays are flattened into `key: value` lines.
pub fn format_value(value: &serde_json::Value) -> String {
    let mut lines = Vec::new();
    collect_lines(value, "", &mut lines);
    lines.join("\n")
}

// ---------------------------------------------------------------------------
// Key lookup
// ---------------------------------------------------------------------------

/// Navigate a JSON value by a dot-separated key path.
/// Returns `None` if any segment is missing or the current node is not an object.
pub fn lookup_key<'a>(
    mut value: &'a serde_json::Value,
    key: &str,
) -> Option<&'a serde_json::Value> {
    for part in key.split('.') {
        value = value.as_object()?.get(part)?;
    }
    Some(value)
}

/// Resolve a translation value supporting both flat and nested JSON, with an
/// optional key prefix.  Mirrors the `resolveTranslation` logic in server.js.
///
/// Lookup order (stops at first hit):
///   1. `data["prefix + key"]`      — flat format with prefix
///   2. dot-traversal(`prefix+key`) — nested format with prefix
///   3. `data["key"]`               — flat format without prefix
///   4. dot-traversal(`key`)        — nested format without prefix
pub fn resolve_translation<'a>(
    data: &'a serde_json::Value,
    raw_key: &str,
    key_prefix: &str,
) -> Option<&'a serde_json::Value> {
    if !key_prefix.is_empty() {
        let full_key = format!("{key_prefix}{raw_key}");

        // 1. Flat lookup with prefix
        if let Some(v) = data.as_object().and_then(|o| o.get(&full_key)) {
            return Some(v);
        }
        // 2. Nested dot-notation with prefix
        if let Some(v) = lookup_key(data, &full_key) {
            return Some(v);
        }
    }

    // 3. Flat lookup without prefix
    if let Some(v) = data.as_object().and_then(|o| o.get(raw_key)) {
        return Some(v);
    }
    // 4. Nested dot-notation without prefix
    lookup_key(data, raw_key)
}

// ---------------------------------------------------------------------------
// Key enumeration
// ---------------------------------------------------------------------------

/// Collect all dot-notation leaf keys from a JSON value into `keys`.
pub fn collect_keys(value: &serde_json::Value, prefix: &str, keys: &mut Vec<String>) {
    match value {
        serde_json::Value::Object(map) => {
            for (k, v) in map {
                let full_key = if prefix.is_empty() {
                    k.clone()
                } else {
                    format!("{prefix}.{k}")
                };
                collect_keys(v, &full_key, keys);
            }
        }
        serde_json::Value::Array(arr) => {
            for (i, v) in arr.iter().enumerate() {
                let indexed_key = if prefix.is_empty() {
                    format!("[{i}]")
                } else {
                    format!("{prefix}[{i}]")
                };
                collect_keys(v, &indexed_key, keys);
            }
        }
        _ => {
            // Leaf node — record the accumulated key path.
            if !prefix.is_empty() {
                keys.push(prefix.to_string());
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Entry enumeration (key + value pairs)
// ---------------------------------------------------------------------------

/// Collect all leaf `(key, value)` pairs from a JSON value into `entries`.
/// `prefix` is the dot-path accumulated so far (empty for the root call).
pub fn collect_entries(
    value: &serde_json::Value,
    prefix: &str,
    entries: &mut Vec<(String, String)>,
) {
    match value {
        serde_json::Value::Object(map) => {
            for (k, v) in map {
                let full_key = if prefix.is_empty() {
                    k.clone()
                } else {
                    format!("{prefix}.{k}")
                };
                collect_entries(v, &full_key, entries);
            }
        }
        serde_json::Value::Array(arr) => {
            for (i, v) in arr.iter().enumerate() {
                let indexed_key = if prefix.is_empty() {
                    format!("[{i}]")
                } else {
                    format!("{prefix}[{i}]")
                };
                collect_entries(v, &indexed_key, entries);
            }
        }
        serde_json::Value::String(s) => {
            if !prefix.is_empty() {
                entries.push((prefix.to_string(), s.clone()));
            }
        }
        serde_json::Value::Number(n) => {
            if !prefix.is_empty() {
                entries.push((prefix.to_string(), n.to_string()));
            }
        }
        serde_json::Value::Bool(b) => {
            if !prefix.is_empty() {
                entries.push((prefix.to_string(), b.to_string()));
            }
        }
        serde_json::Value::Null => {
            if !prefix.is_empty() {
                entries.push((prefix.to_string(), "(null)".to_string()));
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Fuzzy matching
// ---------------------------------------------------------------------------

/// Fuzzy-match `query` against `text` and return a relevance score.
///
/// Scoring tiers (higher = better match):
///   10000  — exact case-insensitive match
///    5000+ — substring match (bonus for position & coverage)
///    3000  — all whitespace-separated query words appear in text
///     1+   — all query characters appear in order (classic fuzzy)
///
/// Returns `None` if no match is found.
pub fn fuzzy_score(query: &str, text: &str) -> Option<u32> {
    if query.is_empty() {
        return None;
    }

    let q = query.to_lowercase();
    let t = text.to_lowercase();

    // Tier 1: exact match
    if t == q {
        return Some(10000);
    }

    // Tier 2: substring match
    if let Some(pos) = t.find(q.as_str()) {
        // Coverage: how much of the text the query spans (0–100)
        let coverage = (q.len() * 100 / t.len().max(1)) as u32;
        // Position bonus: earlier matches score higher
        let position_bonus = 100u32.saturating_sub(pos as u32);
        return Some(5000 + coverage * 10 + position_bonus);
    }

    // Tier 3: all query words appear somewhere in the text (any order)
    let words: Vec<&str> = q.split_whitespace().collect();
    if words.len() > 1 && words.iter().all(|w| t.contains(w)) {
        return Some(3000);
    }

    // Tier 4: all query characters appear in order (classic fuzzy)
    let mut qi = q.chars().peekable();
    let mut score: u32 = 0;
    let mut consecutive: u32 = 0;
    let mut first_match_pos: Option<usize> = None;

    for (pos, c) in t.chars().enumerate() {
        if qi.peek() == Some(&c) {
            qi.next();
            consecutive += 1;
            // Consecutive character runs get an exponential bonus
            score += consecutive * 2;
            if first_match_pos.is_none() {
                first_match_pos = Some(pos);
            }
        } else {
            consecutive = 0;
        }
    }

    if qi.peek().is_none() {
        // Earlier first match = higher score
        let pos_bonus = 50u32.saturating_sub(first_match_pos.unwrap_or(0) as u32);
        Some(score + pos_bonus)
    } else {
        None
    }
}
