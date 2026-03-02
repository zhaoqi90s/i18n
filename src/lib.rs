use zed_extension_api::{
    self as zed, serde_json, SlashCommand, SlashCommandArgumentCompletion, SlashCommandOutput,
    SlashCommandOutputSection, Worktree,
};

// ---------------------------------------------------------------------------
// Common language code completions
// ---------------------------------------------------------------------------

const COMMON_LANGS: &[(&str, &str)] = &[
    ("en", "English"),
    ("zh", "Chinese (Simplified)"),
    ("zh-TW", "Chinese (Traditional)"),
    ("ja", "Japanese"),
    ("ko", "Korean"),
    ("de", "German"),
    ("fr", "French"),
    ("es", "Spanish"),
    ("pt", "Portuguese"),
    ("pt-BR", "Portuguese (Brazil)"),
    ("ru", "Russian"),
    ("ar", "Arabic"),
    ("it", "Italian"),
    ("nl", "Dutch"),
    ("pl", "Polish"),
    ("tr", "Turkish"),
    ("vi", "Vietnamese"),
    ("th", "Thai"),
    ("id", "Indonesian"),
    ("cs", "Czech"),
    ("sv", "Swedish"),
    ("da", "Danish"),
    ("fi", "Finnish"),
    ("nb", "Norwegian"),
    ("he", "Hebrew"),
    ("uk", "Ukrainian"),
    ("hu", "Hungarian"),
    ("ro", "Romanian"),
    ("el", "Greek"),
    ("bg", "Bulgarian"),
    ("hr", "Croatian"),
    ("sk", "Slovak"),
    ("lt", "Lithuanian"),
    ("lv", "Latvian"),
    ("et", "Estonian"),
    ("sl", "Slovenian"),
    ("ca", "Catalan"),
    ("ms", "Malay"),
    ("fa", "Persian"),
    ("hi", "Hindi"),
    ("bn", "Bengali"),
];

// ---------------------------------------------------------------------------
// Default file-path templates
// `{lang}` is replaced with the actual language code at runtime.
// ---------------------------------------------------------------------------

const DEFAULT_PATH_TEMPLATES: &[&str] = &[
    // Generic flat structures
    "locales/{lang}.json",
    "locale/{lang}.json",
    "i18n/{lang}.json",
    "translations/{lang}.json",
    "lang/{lang}.json",
    // i18next / react-i18next (public/locales/{lang}/namespace.json)
    "public/locales/{lang}/translation.json",
    "public/locales/{lang}/common.json",
    "public/locales/{lang}/index.json",
    // Nested locales directory
    "locales/{lang}/translation.json",
    "locales/{lang}/index.json",
    "locales/{lang}/common.json",
    "locale/{lang}/translation.json",
    // Source-tree locations
    "src/locales/{lang}.json",
    "src/i18n/{lang}.json",
    "src/i18n/locales/{lang}.json",
    "src/assets/locales/{lang}.json",
    "assets/locales/{lang}.json",
    "assets/i18n/{lang}.json",
    // Flutter ARB (JSON-compatible)
    "lib/l10n/app_{lang}.arb",
];

// ---------------------------------------------------------------------------
// Config loaded from an optional `.i18n-viewer.json` at the worktree root
// ---------------------------------------------------------------------------

struct Config {
    default_lang: String,
    path_templates: Vec<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            default_lang: "en".to_string(),
            path_templates: DEFAULT_PATH_TEMPLATES
                .iter()
                .map(|s| s.to_string())
                .collect(),
        }
    }
}

fn load_config(worktree: &Worktree) -> Config {
    let content = match worktree.read_text_file(".i18n-viewer.json") {
        Ok(c) => c,
        Err(_) => return Config::default(),
    };

    let v: serde_json::Value = match serde_json::from_str(&content) {
        Ok(v) => v,
        Err(_) => return Config::default(),
    };

    let default_lang = v
        .get("defaultLang")
        .and_then(|s| s.as_str())
        .unwrap_or("en")
        .to_string();

    let path_templates = v
        .get("paths")
        .and_then(|p| p.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|item| item.as_str().map(String::from))
                .collect::<Vec<_>>()
        })
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| {
            DEFAULT_PATH_TEMPLATES
                .iter()
                .map(|s| s.to_string())
                .collect()
        });

    Config {
        default_lang,
        path_templates,
    }
}

// ---------------------------------------------------------------------------
// Helpers: path resolution
// ---------------------------------------------------------------------------

fn resolve_paths(lang: &str, templates: &[String]) -> Vec<String> {
    templates
        .iter()
        .map(|t| t.replace("{lang}", lang))
        .collect()
}

/// Try each candidate path in order; return the first readable content plus
/// which path succeeded. If none succeed, return all tried paths for the error.
fn find_translation_file(
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

// ---------------------------------------------------------------------------
// Helpers: JSON value formatting
// ---------------------------------------------------------------------------

/// Recursively walk a JSON value and produce flat `key: value` lines.
/// `prefix` is the dot-path accumulated so far (empty for the root call).
fn collect_lines(value: &serde_json::Value, prefix: &str, lines: &mut Vec<String>) {
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
/// - Objects/arrays are flattened into `key: value` lines.
fn format_value(value: &serde_json::Value) -> String {
    let mut lines = Vec::new();
    collect_lines(value, "", &mut lines);
    lines.join("\n")
}

// ---------------------------------------------------------------------------
// Helpers: key lookup
// ---------------------------------------------------------------------------

/// Navigate a JSON value by a dot-separated key path.
/// Returns `None` if any segment is missing or the current node is not an object.
fn lookup_key<'a>(mut value: &'a serde_json::Value, key: &str) -> Option<&'a serde_json::Value> {
    for part in key.split('.') {
        value = value.as_object()?.get(part)?;
    }
    Some(value)
}

// ---------------------------------------------------------------------------
// Command implementations
// ---------------------------------------------------------------------------

fn run_i18n(args: Vec<String>, worktree: Option<&Worktree>) -> Result<SlashCommandOutput, String> {
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

    // --- find translation file ---
    let candidates = resolve_paths(&lang, &config.path_templates);
    let (content, found_path) = find_translation_file(worktree, &candidates).map_err(|tried| {
        let list = tried
            .iter()
            .map(|p| format!("  • {p}"))
            .collect::<Vec<_>>()
            .join("\n");
        format!(
            concat!(
                "No translation file found for language '{lang}'.\n\n",
                "Searched:\n{list}\n\n",
                "Tip: create a .i18n-viewer.json at your project root to configure custom paths:\n",
                "{{\n",
                "  \"defaultLang\": \"{lang}\",\n",
                "  \"paths\": [\"your/custom/path/{{lang}}.json\"]\n",
                "}}"
            ),
            lang = lang,
            list = list,
        )
    })?;

    // --- parse JSON ---
    let json: serde_json::Value = serde_json::from_str(&content)
        .map_err(|e| format!("Failed to parse '{found_path}': {e}"))?;

    // --- look up key ---
    let value =
        lookup_key(&json, key).ok_or_else(|| format!("Key '{key}' not found in '{found_path}'"))?;

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

    let header = format!("i18n: {key} [{lang}]{kind_hint}");
    let rule = "─".repeat(header.len().min(72));
    let source_line = format!("(from {found_path})");
    let text = format!("{header}\n{rule}\n{translation}\n\n{source_line}");
    let label = format!("i18n: {key} [{lang}]");

    Ok(SlashCommandOutput {
        sections: vec![SlashCommandOutputSection {
            range: (0..text.len()).into(),
            label,
        }],
        text,
    })
}

fn run_i18n_keys(
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

    // --- find translation file ---
    let candidates = resolve_paths(&lang, &config.path_templates);
    let (content, found_path) = find_translation_file(worktree, &candidates).map_err(|tried| {
        let list = tried
            .iter()
            .map(|p| format!("  • {p}"))
            .collect::<Vec<_>>()
            .join("\n");
        format!("No translation file found for language '{lang}'.\n\nSearched:\n{list}")
    })?;

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

/// Collect all dot-notation leaf keys from a JSON object into `keys`.
fn collect_keys(value: &serde_json::Value, prefix: &str, keys: &mut Vec<String>) {
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
            // Leaf node — record the key
            if !prefix.is_empty() {
                keys.push(prefix.to_string());
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Extension struct & trait impl
// ---------------------------------------------------------------------------

struct I18nTranslatorExtension;

impl zed::Extension for I18nTranslatorExtension {
    fn new() -> Self {
        I18nTranslatorExtension
    }

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
            // /i18n <key> [lang]
            // The first arg is the key (no useful completions we can offer without
            // reading the file). The second arg is the language code.
            "i18n" => {
                if args.len() >= 1 {
                    // User has typed the key; now completing the language
                    Ok(lang_completions())
                } else {
                    Ok(vec![])
                }
            }

            // /i18n-keys [lang]
            // The only (optional) arg is the language code.
            "i18n-keys" => Ok(lang_completions()),

            unknown => Err(format!("unknown slash command: \"{unknown}\"")),
        }
    }

    fn run_slash_command(
        &self,
        command: SlashCommand,
        args: Vec<String>,
        worktree: Option<&Worktree>,
    ) -> Result<SlashCommandOutput, String> {
        match command.name.as_str() {
            "i18n" => run_i18n(args, worktree),
            "i18n-keys" => run_i18n_keys(args, worktree),
            unknown => Err(format!("unknown slash command: \"{unknown}\"")),
        }
    }
}

zed::register_extension!(I18nTranslatorExtension);
