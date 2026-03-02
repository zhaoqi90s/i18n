# i18n Key Translator

A [Zed](https://zed.dev) extension that lets you look up i18n translation keys
directly from the Assistant panel via slash commands.

## Commands

### `/i18n <key> [lang]`

Look up the translation for a given key in the specified language.

```
/i18n common.button.save
/i18n common.button.save zh
/i18n auth.errors.invalidEmail fr
```

- `key` — dot-notation path to the translation (e.g. `common.button.save`)
- `lang` — BCP 47 language code (optional, defaults to `en` or your configured `defaultLang`)

When a key points to a **namespace** (an object rather than a string), all
leaf key-value pairs under that namespace are shown:

```
/i18n common.button zh
─────────────────────────────────────────
save: 保存
cancel: 取消
confirm: 确认
```

### `/i18n-keys [lang]`

List every dot-notation key in the translation file for a given language.
Useful for discovering what keys are available before looking them up.

```
/i18n-keys
/i18n-keys zh
```

## Supported file layouts

The extension searches a set of well-known paths automatically.
`{lang}` is replaced with the language code you provide.

| Pattern | Frameworks |
|---|---|
| `locales/{lang}.json` | Generic, Vue i18n |
| `locale/{lang}.json` | Generic |
| `i18n/{lang}.json` | Generic |
| `translations/{lang}.json` | Generic |
| `lang/{lang}.json` | Generic |
| `public/locales/{lang}/translation.json` | i18next / react-i18next |
| `public/locales/{lang}/common.json` | i18next |
| `locales/{lang}/translation.json` | i18next |
| `src/locales/{lang}.json` | Vue i18n, Angular |
| `src/i18n/{lang}.json` | Angular |
| `src/assets/locales/{lang}.json` | Angular |
| `assets/i18n/{lang}.json` | Angular |
| `lib/l10n/app_{lang}.arb` | Flutter |

If your project uses a different layout, see [Custom configuration](#custom-configuration) below.

## Custom configuration

Create a `.i18n-viewer.json` file at the **root of your project** to override
the defaults:

```json
{
  "defaultLang": "zh",
  "paths": [
    "resources/lang/{lang}.json",
    "config/i18n/{lang}/messages.json"
  ]
}
```

| Field | Type | Default | Description |
|---|---|---|---|
| `defaultLang` | `string` | `"en"` | Language used when no `[lang]` argument is given |
| `paths` | `string[]` | (built-in list) | Ordered list of path templates to try. `{lang}` is substituted at runtime. If set, the built-in list is **replaced** (not merged). |

## Installation (dev extension)

> **Prerequisites:** [Rust must be installed via `rustup`](https://rustup.rs).
> Installing via Homebrew or other means will not work with Zed dev extensions.

1. Clone this repository:
   ```sh
   git clone https://github.com/yourusername/i18n-key-translator
   ```

2. Open Zed and press `Cmd+Shift+X` (macOS) or `Ctrl+Shift+X` (Linux/Windows)
   to open the Extensions panel.

3. Click **Install Dev Extension** and select the cloned directory.

4. Zed will compile the extension to WebAssembly — this takes a moment on the
   first build.

5. Open the Assistant panel (`Cmd+?` or `Ctrl+?`) and type `/i18n` to start.

### Rebuilding after changes

After editing the source, click **Rebuild** next to the extension in the
Extensions panel, or re-run **Install Dev Extension**.

## Troubleshooting

### The command returns "No translation file found"

1. Check that your project is open as a folder (not just a single file).
2. Run `/i18n-keys` without a key to see which paths were tried.
3. Add a `.i18n-viewer.json` with the correct `paths` for your project.

### Parsing errors

The extension expects standard JSON. Make sure your translation file:
- Has no trailing commas
- Uses UTF-8 encoding
- Is not a YAML or TOML file (only JSON and `.arb` are supported)

### Build fails / Rust errors

- Ensure Rust was installed via `rustup` (not Homebrew).
- Run `rustup target add wasm32-wasip2` if the target is missing.
- Check `Zed: Open Log` in the command palette for detailed error output.
- Launching Zed with `zed --foreground` prints extension logs to the terminal.

## Project structure

```
i18n-key-translator/
├── extension.toml        # Extension manifest (id, name, slash commands)
├── Cargo.toml            # Rust crate configuration
├── src/
│   └── lib.rs            # All extension logic (WASM entry point)
├── .i18n-viewer.json     # (optional) per-project config — add to your project, not here
├── LICENSE
└── README.md
```

## License

MIT — see [LICENSE](LICENSE).