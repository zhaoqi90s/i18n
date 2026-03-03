use zed_extension_api::{serde_json, Worktree};

// ---------------------------------------------------------------------------
// Default file-path templates
// `{lang}` is replaced with the actual language code at runtime.
// ---------------------------------------------------------------------------

pub const DEFAULT_PATH_TEMPLATES: &[&str] = &[
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

pub struct Config {
    pub default_lang: String,
    pub cache_dir: String,
    pub local_paths: Vec<String>,
    pub key_prefix: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            default_lang: "en".to_string(),
            cache_dir: ".i18n-cache".to_string(),
            local_paths: DEFAULT_PATH_TEMPLATES
                .iter()
                .map(|s| s.to_string())
                .collect(),
            key_prefix: String::new(),
        }
    }
}

/// Load configuration from `.i18n-viewer.json` at the worktree root.
/// Falls back to [`Config::default`] if the file is absent or malformed.
pub fn load_config(worktree: &Worktree) -> Config {
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

    let cache_dir = v
        .get("cacheDir")
        .and_then(|s| s.as_str())
        .unwrap_or(".i18n-cache")
        .to_string();

    // Accepts both "localPaths" (current schema) and legacy "paths".
    let local_paths = v
        .get("localPaths")
        .or_else(|| v.get("paths"))
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

    let key_prefix = v
        .get("keyPrefix")
        .and_then(|s| s.as_str())
        .unwrap_or("")
        .to_string();

    Config {
        default_lang,
        cache_dir,
        local_paths,
        key_prefix,
    }
}
