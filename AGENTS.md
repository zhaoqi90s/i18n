# PROJECT KNOWLEDGE BASE

**Generated:** 2026-03-17
**Commit:** 058e460
**Branch:** main

## OVERVIEW

A [Zed Editor](https://zed.dev) extension for i18n key translation with inline hover previews. Hybrid Rust/WASM + Node.js architecture.

## STRUCTURE

```
i18n/
├── src/           # Rust extension source
│   ├── lib.rs     # Extension entry point
│   ├── config.rs  # Configuration loading
│   ├── langs.rs   # Language constants
│   ├── translation.rs  # Core translation logic
│   └── commands/  # Slash command implementations
├── lsp/           # Node.js LSP server
│   └── server.js  # Hover preview server
├── Cargo.toml     # Rust package manifest
├── extension.toml # Zed extension manifest
└── .i18n-viewer.json.example  # Config template
```

## WHERE TO LOOK

| Task | Location | Notes |
|------|----------|-------|
| Entry point | `src/lib.rs` | Implements `zed::Extension` trait |
| Slash commands | `src/commands/*.rs` | /i18n, /i18n-keys, /i18n-search, /i18n-sync |
| Config loading | `src/config.rs` | `.i18n-viewer.json` parsing |
| Translation logic | `src/translation.rs` | File resolution, key lookup, fuzzy matching |
| LSP server | `lsp/server.js` | Hover preview over stdio |
| Language constants | `src/langs.rs` | COMMON_LANGS for autocomplete |

## CODE MAP

| Symbol | Type | Location | Role |
|--------|------|----------|------|
| `I18nTranslatorExtension` | struct | `src/lib.rs:26` | Main extension struct |
| `register_extension!` | macro | `src/lib.rs:149` | Registers extension with Zed |
| `SERVER_JS` | const | `src/lib.rs:20` | Embedded LSP server code |
| `load_config()` | fn | `src/config.rs:62` | Loads `.i18n-viewer.json` |
| `find_translation()` | fn | `src/translation.rs:39` | Resolves translation files |
| `resolve_translation()` | fn | `src/translation.rs:163` | Looks up keys with prefix support |
| `fuzzy_score()` | fn | `src/translation.rs:293` | Fuzzy matching for search |
| `run_i18n()` | fn | `src/commands/i18n.rs:7` | /i18n command handler |
| `COMMON_LANGS` | const | `src/langs.rs:3` | Language code mappings |

## CONVENTIONS

### Code Style
- **Rust**: Standard Rust naming (snake_case for functions/variables, PascalCase for types)
- **Comments**: Section headers use `// --- Section Name ---` format
- **Documentation**: `///` for public API docs

### Architecture Patterns
- **WASM Extension**: Cannot make HTTP requests directly; delegates to LSP server via IPC file
- **LSP Server**: Uses only Node.js built-in modules (no npm dependencies)
- **File Resolution**: Cache-first (`.i18n-cache/`), then local paths
- **Key Lookup**: 4-tier fallback (flat+prefix → nested+prefix → flat → nested)

### Configuration
- User config: `.i18n-viewer.json` at project root
- Legacy key `paths` accepted but deprecated (use `localPaths`)
- Patterns array **replaces** (not extends) built-in patterns

## ANTI-PATTERNS

### DO NOT
- Add npm dependencies to `lsp/server.js` (breaks zero-config deployment)
- Make HTTP requests from Rust WASM (sandboxed; use IPC file instead)
- Forget to rebuild WASM after editing `lsp/server.js` (embedded via `include_str!`)
- Commit `.i18n-cache/` directory (add to `.gitignore`)
- Use trailing commas in JSON files (invalid for standard JSON parser)

### NEVER
- Use `Box<dyn Error>` for error handling (project uses `Result<T, String>`)
- Mix async/await in LSP server (purely synchronous Node.js code)
- Expect `patterns` to extend built-ins (it **replaces** them entirely)

### ALWAYS
- Use `cargo check --target wasm32-wasip2` before full build (faster feedback)
- Run `node --check lsp/server.js` to verify LSP syntax
- Handle missing config gracefully (fallback to defaults in `src/config.rs`)
- Use 4-tier key lookup strategy for maximum compatibility

## COMMANDS

### Development
```bash
# Quick compile check (no WASM output, very fast)
cargo check --target wasm32-wasip2

# Verify LSP server syntax
node --check lsp/server.js

# Full WASM build
cargo build --target wasm32-wasip2

# Test LSP server manually
node lsp/server.js
```

### Zed Extension
```bash
# Register extension in Zed:
# Extensions → Install Dev Extension → select this directory

# Reload extension after changes:
# Command Palette → "zed: reload extensions"
```

### Viewing Logs
```bash
# Open Zed log
# Command Palette → "zed: open log"

# Or run Zed in foreground to see logs in terminal
zed --foreground
```

## NOTES

### Build Requirements
- Rust (via rustup, not Homebrew)
- `wasm32-wasip2` target: `rustup target add wasm32-wasip2`
- Node.js (Zed manages its own copy for the extension)

### Deployment
- No CI/CD configured (manual process)
- Deploy by publishing to Zed extension registry

### Hybrid Architecture
- **WASM Extension** (`src/`): Handles slash commands, starts LSP server
- **LSP Server** (`lsp/server.js`): Handles hover preview via textDocument/hover
- **Communication**: IPC via `.sync-request` file (WASM sandbox limitation)

### File Size
- Small project (~17 files, ~2000 lines)
- No test suite (manual testing via development workflow)
