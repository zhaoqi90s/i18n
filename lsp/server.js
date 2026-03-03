#!/usr/bin/env node
"use strict";

/**
 * i18n Key Translator — LSP server
 *
 * Self-contained: uses only Node.js built-in modules (no npm needed).
 * Communicates via JSON-RPC over stdin/stdout per the LSP specification.
 *
 * Features
 * --------
 *  - Reads .i18n-viewer.json from the workspace root for configuration
 *  - Downloads remote translation files (with in-memory + disk cache)
 *  - Falls back to local translation files when no remote source is configured
 *  - Extracts i18n keys from source code using configurable regex patterns
 *  - Returns a Markdown hover card showing the translation for the key under cursor
 *
 * Configuration file (.i18n-viewer.json):
 * {
 *   "defaultLang": "en",
 *   "languages": ["en", "zh", "ja"],          // extra langs shown in hover
 *   "remoteSources": {
 *     "en": "https://example.com/locales/en.json",
 *     "zh": "https://example.com/locales/zh.json"
 *   },
 *   "localPaths": ["locales/{lang}.json"],    // override built-in path search
 *   "patterns": [
 *     "formatMessage\\([\"']([^\"']+)[\"']",
 *     "t\\([\"']([^\"']+)[\"']"
 *   ],
 *   "keyPrefix": "isv-common.language.",      // prepend to extracted keys before lookup
 *   "cacheDir": ".i18n-cache",
 *   "ttl": 3600
 * }
 *
 * Flat JSON support
 * -----------------
 * Translation files may use a flat key/value structure:
 *   { "isv-common.language.ar": "阿拉伯语", "isv-common.language.bg": "保加利亚语" }
 *
 * Combined with `keyPrefix`, a call like formatMessage('ar') will look up:
 *   1. data["isv-common.language.ar"]  — flat key (direct property access)
 *   2. data.isv-common.language.ar     — nested dot-notation traversal (fallback)
 *   3. data["ar"]                      — flat key without prefix (fallback)
 *   4. data.ar                         — nested traversal without prefix (fallback)
 */

const http = require("http");
const https = require("https");
const fs = require("fs");
const path = require("path");

// Path to the sync-request file written by the /i18n-sync slash command.
// The slash command runs inside Zed's WASM sandbox and cannot make HTTP requests
// or write to the workspace directly, so it uses this file as an IPC channel.
const SYNC_REQUEST_PATH = path.join(__dirname, "..", ".sync-request");

// ─── Logging (to stderr so it doesn't corrupt the LSP framing on stdout) ──────

function logInfo(...args) {
  process.stderr.write("[i18n-lsp] " + args.join(" ") + "\n");
}
function logError(...args) {
  process.stderr.write("[i18n-lsp] ERROR: " + args.join(" ") + "\n");
}

// ─── JSON-RPC / LSP framing over stdio ───────────────────────────────────────

let _buf = ""; // raw utf-8 accumulation buffer
let _expectedBytes = -1; // pending Content-Length to consume

process.stdin.setEncoding("utf8");
process.stdin.on("data", (chunk) => {
  _buf += chunk;
  _drain();
});
process.stdin.on("end", () => process.exit(0));

function _drain() {
  while (true) {
    // Phase 1: parse headers
    if (_expectedBytes < 0) {
      const sep = _buf.indexOf("\r\n\r\n");
      if (sep < 0) break;
      const headers = _buf.slice(0, sep);
      const m = headers.match(/Content-Length:\s*(\d+)/i);
      if (!m) {
        _buf = _buf.slice(sep + 4);
        continue;
      }
      _expectedBytes = parseInt(m[1], 10);
      _buf = _buf.slice(sep + 4);
    }

    // Phase 2: read body bytes (must be byte-accurate for non-ASCII)
    const bodyBytes = Buffer.from(_buf, "utf8");
    if (bodyBytes.length < _expectedBytes) break;

    const msgStr = bodyBytes.slice(0, _expectedBytes).toString("utf8");
    _buf = bodyBytes.slice(_expectedBytes).toString("utf8");
    _expectedBytes = -1;

    let msg;
    try {
      msg = JSON.parse(msgStr);
    } catch (e) {
      logError("JSON parse error:", e.message);
      continue;
    }

    handleMessage(msg).catch((e) =>
      logError(
        "handleMessage threw:",
        e && (e.stack || e.message || String(e)),
      ),
    );
  }
}

function _send(msg) {
  const json = JSON.stringify(msg);
  const bytes = Buffer.byteLength(json, "utf8");
  process.stdout.write(`Content-Length: ${bytes}\r\n\r\n${json}`);
}

function respond(id, result) {
  _send({ jsonrpc: "2.0", id, result });
}
function respondError(id, code, message) {
  _send({ jsonrpc: "2.0", id, error: { code, message } });
}

// ─── State ────────────────────────────────────────────────────────────────────

let workspacePath = null; // absolute path of the open project root
let cfg = null; // resolved configuration object

/** In-memory translation cache: lang → { data: Object, at: Date.now() ms } */
const memCache = Object.create(null);

/** Currently open document texts: uri → string */
const docs = Object.create(null);

// ─── Defaults ─────────────────────────────────────────────────────────────────

const DEFAULTS = {
  defaultLang: "en",
  languages: [],
  remoteSources: {},
  localPaths: [],
  patterns: [
    "formatMessage\\([\"']([^\"']+)[\"']",
    "\\$t\\([\"']([^\"']+)[\"']",
    "\\bt\\([\"']([^\"']+)[\"']",
    "i18n\\.t\\([\"']([^\"']+)[\"']",
    "i18n\\([\"']([^\"']+)[\"']",
    "gettext\\([\"']([^\"']+)[\"']",
    "translate\\([\"']([^\"']+)[\"']",
    "intl\\.formatMessage\\(\\{[^}]*id:\\s*[\"']([^\"']+)[\"']",
    "<Trans[^>]+i18nKey=[\"']([^\"']+)[\"']",
  ],
  keyPrefix: "",
  cacheDir: ".i18n-cache",
  ttl: 3600,
};

const FALLBACK_LOCAL_TEMPLATES = [
  "locales/{lang}.json",
  "locale/{lang}.json",
  "i18n/{lang}.json",
  "translations/{lang}.json",
  "lang/{lang}.json",
  "public/locales/{lang}/translation.json",
  "public/locales/{lang}/common.json",
  "public/locales/{lang}/index.json",
  "locales/{lang}/translation.json",
  "locales/{lang}/index.json",
  "locales/{lang}/common.json",
  "src/locales/{lang}.json",
  "src/i18n/{lang}.json",
  "src/i18n/locales/{lang}.json",
  "src/assets/locales/{lang}.json",
  "assets/locales/{lang}.json",
  "assets/i18n/{lang}.json",
];

// ─── Configuration loading ────────────────────────────────────────────────────

function loadConfig() {
  const defaults = Object.assign({}, DEFAULTS);
  if (!workspacePath) return defaults;

  const cfgPath = path.join(workspacePath, ".i18n-viewer.json");
  let raw;
  try {
    raw = JSON.parse(fs.readFileSync(cfgPath, "utf8"));
  } catch (_) {
    return defaults;
  }

  return {
    defaultLang: raw.defaultLang || defaults.defaultLang,
    languages: Array.isArray(raw.languages)
      ? raw.languages
      : defaults.languages,
    remoteSources:
      raw.remoteSources && typeof raw.remoteSources === "object"
        ? raw.remoteSources
        : defaults.remoteSources,
    localPaths:
      Array.isArray(raw.localPaths) && raw.localPaths.length > 0
        ? raw.localPaths
        : defaults.localPaths,
    patterns:
      Array.isArray(raw.patterns) && raw.patterns.length > 0
        ? raw.patterns
        : defaults.patterns,
    keyPrefix:
      typeof raw.keyPrefix === "string" ? raw.keyPrefix : defaults.keyPrefix,
    cacheDir: raw.cacheDir || defaults.cacheDir,
    ttl: typeof raw.ttl === "number" ? raw.ttl : defaults.ttl,
  };
}

// ─── HTTP/HTTPS helper ────────────────────────────────────────────────────────

function fetchUrl(url) {
  return new Promise((resolve, reject) => {
    const mod = url.startsWith("https") ? https : http;
    const req = mod.get(url, { timeout: 10000 }, (res) => {
      if (
        res.statusCode >= 300 &&
        res.statusCode < 400 &&
        res.headers.location
      ) {
        // follow one redirect
        return fetchUrl(res.headers.location).then(resolve).catch(reject);
      }
      if (res.statusCode !== 200) {
        return reject(new Error(`HTTP ${res.statusCode} for ${url}`));
      }
      const chunks = [];
      res.on("data", (c) => chunks.push(c));
      res.on("end", () => {
        try {
          resolve(JSON.parse(Buffer.concat(chunks).toString("utf8")));
        } catch (e) {
          reject(new Error("JSON parse failed for " + url + ": " + e.message));
        }
      });
    });
    req.on("error", reject);
    req.on("timeout", () => {
      req.destroy();
      reject(new Error("Request timed out: " + url));
    });
  });
}

// ─── Leaf-key counter (for sync summary) ─────────────────────────────────────

function countLeafKeys(obj) {
  if (typeof obj !== "object" || obj === null || Array.isArray(obj)) return 1;
  let n = 0;
  for (const v of Object.values(obj)) n += countLeafKeys(v);
  return n || 1;
}

// ─── Manual sync: clear cache + re-fetch remote sources ──────────────────────

/**
 * Download remote translation files for the given language codes, update the
 * in-memory cache, and persist them to the workspace disk cache.
 *
 * @param {string[]} langs  Language codes to sync (e.g. ["en", "zh"]).
 * @returns {Promise<Array<{lang, status, keys?, url?, error?, reason?}>>}
 */
async function syncTranslations(langs) {
  const config = cfg || DEFAULTS;
  const results = [];

  for (const lang of langs) {
    const remoteUrl = config.remoteSources[lang];
    if (!remoteUrl) {
      results.push({
        lang,
        status: "skip",
        reason: "no remote source configured",
      });
      continue;
    }

    // ── 1. Evict in-memory cache ─────────────────────────────────────────────
    delete memCache[lang];

    // ── 2. Delete disk cache so getTranslations won't serve a stale file ─────
    const diskPath = workspacePath
      ? path.join(workspacePath, config.cacheDir, lang + ".json")
      : null;

    if (diskPath) {
      try {
        fs.unlinkSync(diskPath);
      } catch (_) {
        /* file may not exist yet – that's fine */
      }
    }

    // ── 3. Re-fetch ──────────────────────────────────────────────────────────
    try {
      logInfo(`[sync] Fetching ${lang} from ${remoteUrl}`);
      const data = await fetchUrl(remoteUrl);
      const keys = countLeafKeys(data);

      memCache[lang] = { data, at: Date.now() };

      if (diskPath) {
        try {
          fs.mkdirSync(path.dirname(diskPath), { recursive: true });
          fs.writeFileSync(diskPath, JSON.stringify(data, null, 2), "utf8");
          logInfo(`[sync] Cached ${lang} → ${diskPath} (${keys} keys)`);
        } catch (e) {
          logError(`[sync] Could not write disk cache for ${lang}:`, e.message);
        }
      }

      results.push({ lang, status: "ok", keys, url: remoteUrl });
    } catch (e) {
      logError(`[sync] Fetch failed for ${lang}:`, e.message);
      results.push({ lang, status: "error", error: e.message, url: remoteUrl });
    }
  }

  return results;
}

// ─── Translation loading with caching ────────────────────────────────────────

async function getTranslations(lang) {
  const config = cfg || DEFAULTS;
  const ttlMs = config.ttl * 1000;

  // 1. Hot in-memory cache
  const hit = memCache[lang];
  if (hit && Date.now() - hit.at < ttlMs) return hit.data;

  // 2. Disk cache (within the workspace's cacheDir)
  const diskPath = workspacePath
    ? path.join(workspacePath, config.cacheDir, lang + ".json")
    : null;

  if (diskPath) {
    try {
      const stat = fs.statSync(diskPath);
      if (Date.now() - stat.mtimeMs < ttlMs) {
        const data = JSON.parse(fs.readFileSync(diskPath, "utf8"));
        memCache[lang] = { data, at: stat.mtimeMs };
        return data;
      }
    } catch (_) {
      /* cache miss */
    }
  }

  // 3. Remote source
  const remoteUrl = config.remoteSources[lang];
  if (remoteUrl) {
    try {
      logInfo("Fetching remote translations for", lang, "from", remoteUrl);
      const data = await fetchUrl(remoteUrl);
      memCache[lang] = { data, at: Date.now() };

      if (diskPath) {
        try {
          fs.mkdirSync(path.dirname(diskPath), { recursive: true });
          fs.writeFileSync(diskPath, JSON.stringify(data), "utf8");
          logInfo("Cached", lang, "to", diskPath);
        } catch (e) {
          logError("Could not write disk cache:", e.message);
        }
      }

      return data;
    } catch (e) {
      logError("Remote fetch failed for", lang + ":", e.message);
      // fall through to local
    }
  }

  // 4. Local files
  if (workspacePath) {
    const templates =
      config.localPaths.length > 0
        ? config.localPaths
        : FALLBACK_LOCAL_TEMPLATES;

    for (const tpl of templates) {
      const filePath = path.join(workspacePath, tpl.replace(/\{lang\}/g, lang));
      try {
        const data = JSON.parse(fs.readFileSync(filePath, "utf8"));
        logInfo("Loaded local translations for", lang, "from", filePath);
        memCache[lang] = { data, at: Date.now() };
        return data;
      } catch (_) {
        /* try next */
      }
    }
  }

  logInfo("No translations found for", lang);
  return null;
}

// ─── Key extraction ───────────────────────────────────────────────────────────

/**
 * Scan the given source line for an i18n key whose string token spans
 * the given character column. Returns the key string or null.
 */
function extractKeyAtPosition(text, line, character) {
  const lines = text.split("\n");
  if (line >= lines.length) return null;
  const srcLine = lines[line];

  const config = cfg || DEFAULTS;
  const patterns = config.patterns;

  for (const patStr of patterns) {
    let regex;
    try {
      regex = new RegExp(patStr, "g");
    } catch (e) {
      logError("Invalid pattern:", patStr, e.message);
      continue;
    }

    let match;
    while ((match = regex.exec(srcLine)) !== null) {
      // The first capture group is the i18n key
      if (match[1] === undefined) continue;

      const keyStart = match.index + match[0].indexOf(match[1]);
      const keyEnd = keyStart + match[1].length;

      if (character >= keyStart && character <= keyEnd) {
        return match[1];
      }
    }
  }

  return null;
}

// ─── Dot-path key lookup ──────────────────────────────────────────────────────

function lookupKey(data, key) {
  const parts = key.split(".");
  let cur = data;
  for (const part of parts) {
    if (cur === null || typeof cur !== "object") return undefined;
    if (!Object.prototype.hasOwnProperty.call(cur, part)) return undefined;
    cur = cur[part];
  }
  return cur;
}

/**
 * Resolve a translation value from data, supporting both flat and nested JSON.
 *
 * Lookup order (stops at first hit):
 *   1. data[prefix + key]        — flat format with prefix
 *   2. dot-traversal(prefix+key) — nested format with prefix
 *   3. data[key]                 — flat format without prefix
 *   4. dot-traversal(key)        — nested format without prefix
 *
 * @param {object} data       - parsed translation JSON (flat or nested)
 * @param {string} rawKey     - key extracted from source (e.g. "ar")
 * @param {string} keyPrefix  - prefix from config (e.g. "isv-common.language.")
 * @returns {*} translation value, or undefined if not found
 */
function resolveTranslation(data, rawKey, keyPrefix) {
  if (data == null) return undefined;

  const prefix = typeof keyPrefix === "string" ? keyPrefix : "";

  if (prefix) {
    const fullKey = prefix + rawKey;

    // 1. Flat lookup with prefix
    if (Object.prototype.hasOwnProperty.call(data, fullKey)) {
      return data[fullKey];
    }

    // 2. Nested dot-notation with prefix
    const nested = lookupKey(data, fullKey);
    if (nested !== undefined) return nested;
  }

  // 3. Flat lookup without prefix
  if (Object.prototype.hasOwnProperty.call(data, rawKey)) {
    return data[rawKey];
  }

  // 4. Nested dot-notation without prefix
  return lookupKey(data, rawKey);
}

function formatValue(value) {
  if (typeof value === "string") return value;
  if (typeof value === "number" || typeof value === "boolean")
    return String(value);
  if (value === null) return "*(null)*";
  if (typeof value === "object")
    return "```json\n" + JSON.stringify(value, null, 2) + "\n```";
  return String(value);
}

// ─── Hover content builder ────────────────────────────────────────────────────

async function buildHoverMarkdown(key) {
  const config = cfg || DEFAULTS;
  const primary = config.defaultLang;
  const keyPrefix = config.keyPrefix || "";

  // The display key shown in the hover card (always the raw extracted key)
  // The full lookup key includes the optional prefix
  const lookupFullKey = keyPrefix ? keyPrefix + key : key;

  // Collect languages to display (primary first, then extras)
  const extras = (config.languages || []).filter((l) => l !== primary);
  const langs = [primary, ...extras];

  // Load all translations in parallel
  const results = await Promise.all(
    langs.map(async (lang) => {
      try {
        const data = await getTranslations(lang);
        const value =
          data != null ? resolveTranslation(data, key, keyPrefix) : undefined;
        return { lang, value };
      } catch (e) {
        return { lang, value: undefined, error: e.message };
      }
    }),
  );

  const found = results.filter((r) => r.value !== undefined);
  if (found.length === 0) {
    return `**i18n:** \`${lookupFullKey}\`\n\n> *(key not found)*`;
  }

  const lines = [`**i18n key:** \`${lookupFullKey}\`\n`];
  for (const { lang, value } of found) {
    lines.push(`**[${lang}]** ${formatValue(value)}`);
  }

  return lines.join("\n\n");
}
// ─── URI → file path helper ───────────────────────────────────────────────────

function uriToPath(uri) {
  if (!uri) return null;
  if (uri.startsWith("file://")) {
    let p = uri.slice("file://".length);
    // On Windows the URI is file:///C:/... — keep the leading slash removal
    if (process.platform === "win32" && p.startsWith("/")) p = p.slice(1);
    return decodeURIComponent(p);
  }
  return uri;
}

// ─── Message dispatcher ───────────────────────────────────────────────────────

async function handleMessage(msg) {
  const { method, id, params } = msg;

  switch (method) {
    // ── Lifecycle ──────────────────────────────────────────────────────────────

    case "initialize": {
      // Resolve the workspace root from whatever the client sends
      const rootUri =
        params &&
        (params.rootUri ||
          (params.workspaceFolders &&
            params.workspaceFolders[0] &&
            params.workspaceFolders[0].uri));
      const rootPath = params && params.rootPath;

      if (rootUri) {
        workspacePath = uriToPath(rootUri);
      } else if (rootPath) {
        workspacePath = rootPath;
      }

      logInfo("Workspace:", workspacePath || "(none)");
      cfg = loadConfig();
      logInfo(
        "Config loaded. defaultLang:",
        cfg.defaultLang,
        "| remotes:",
        Object.keys(cfg.remoteSources).join(",") || "(none)",
        "| patterns:",
        cfg.patterns.length,
        "| keyPrefix:",
        cfg.keyPrefix ? `"${cfg.keyPrefix}"` : "(none)",
      );

      respond(id, {
        capabilities: {
          hoverProvider: true,
          textDocumentSync: {
            openClose: true,
            change: 1, // 1 = Full sync
          },
          executeCommandProvider: {
            commands: ["i18n.sync"],
          },
        },
        serverInfo: { name: "i18n-key-translator-lsp", version: "0.2.0" },
      });
      break;
    }

    case "initialized":
      // Notification — no response required
      break;

    case "shutdown":
      respond(id, null);
      break;

    case "exit":
      process.exit(0);
      break;

    // ── Document sync ──────────────────────────────────────────────────────────

    case "textDocument/didOpen": {
      const td = params && params.textDocument;
      if (td) docs[td.uri] = td.text;
      break;
    }

    case "textDocument/didChange": {
      const td = params && params.textDocument;
      const changes = params && params.contentChanges;
      if (td && changes && changes.length > 0) {
        docs[td.uri] = changes[changes.length - 1].text;
      }
      break;
    }

    case "textDocument/didClose": {
      const td = params && params.textDocument;
      if (td) delete docs[td.uri];
      break;
    }

    // ── Workspace events ───────────────────────────────────────────────────────

    case "workspace/didChangeConfiguration":
      // Reload config in case the user edited .i18n-viewer.json
      cfg = loadConfig();
      // Invalidate mem cache so next hover re-fetches
      for (const k of Object.keys(memCache)) delete memCache[k];
      logInfo("Config reloaded after workspace change.");
      break;

    // ── Hover ──────────────────────────────────────────────────────────────────

    case "textDocument/hover": {
      if (!params || !params.textDocument || !params.position) {
        respond(id, null);
        break;
      }

      const uri = params.textDocument.uri;
      const { line, character } = params.position;
      const text = docs[uri];

      if (!text) {
        respond(id, null);
        break;
      }

      const key = extractKeyAtPosition(text, line, character);
      if (!key) {
        respond(id, null);
        break;
      }

      logInfo("Hover key:", key, "at", line + ":" + character);

      const markdown = await buildHoverMarkdown(key);

      respond(id, {
        contents: { kind: "markdown", value: markdown },
      });
      break;
    }

    // ── Manual sync (workspace/executeCommand → i18n.sync) ────────────────────

    case "workspace/executeCommand": {
      const cmd = params && params.command;
      const cmdArgs = (params && params.arguments) || [];

      if (cmd !== "i18n.sync") {
        if (id !== undefined && id !== null) {
          respondError(id, -32601, `Unknown command: ${cmd}`);
        }
        break;
      }

      const syncConfig = cfg || DEFAULTS;
      const requestedLang = typeof cmdArgs[0] === "string" ? cmdArgs[0] : null;
      const langsToSync = requestedLang
        ? [requestedLang]
        : Object.keys(syncConfig.remoteSources);

      if (langsToSync.length === 0) {
        respond(id, {
          message:
            "No remote sources configured in .i18n-viewer.json — nothing to sync.",
        });
        break;
      }

      logInfo("[sync] Starting sync for:", langsToSync.join(", "));

      try {
        const results = await syncTranslations(langsToSync);
        const ok = results.filter((r) => r.status === "ok");
        const failed = results.filter((r) => r.status === "error");
        const skipped = results.filter((r) => r.status === "skip");

        const summary = [
          ok.length > 0
            ? `Synced: ${ok.map((r) => `${r.lang} (${r.keys} keys)`).join(", ")}`
            : null,
          failed.length > 0
            ? `Failed: ${failed.map((r) => `${r.lang} — ${r.error}`).join("; ")}`
            : null,
          skipped.length > 0
            ? `Skipped (no remote source): ${skipped.map((r) => r.lang).join(", ")}`
            : null,
        ]
          .filter(Boolean)
          .join("\n");

        logInfo("[sync] Done.\n" + summary);
        respond(id, { synced: ok, failed, skipped, summary });
      } catch (e) {
        logError("[sync] Unexpected error:", e.message);
        respondError(id, -32603, "Sync failed: " + e.message);
      }
      break;
    }

    // ── Unknown ────────────────────────────────────────────────────────────────

    default:
      if (id !== undefined && id !== null) {
        respondError(id, -32601, `Method not found: ${method}`);
      }
      break;
  }
}

// ─── Entry point ──────────────────────────────────────────────────────────────

logInfo("i18n Key Translator LSP server started (pid", process.pid + ")");

// ─── Sync-request file polling ────────────────────────────────────────────────
// Polls for a .sync-request file written by the /i18n-sync slash command.
// When found (and matching this server's workspace), consumes the file and
// runs the actual remote fetch + disk-cache write in this Node.js process.

setInterval(async () => {
  if (!workspacePath) return; // not yet initialized

  // Try to read the request file
  let req;
  try {
    req = JSON.parse(fs.readFileSync(SYNC_REQUEST_PATH, "utf8"));
  } catch (_) {
    return; // file absent or malformed — normal case on most ticks
  }

  // Each server instance is bound to one workspace; ignore foreign requests
  if (req.workspace !== workspacePath) return;

  // Consume atomically — first unlink wins if multiple instances race
  try {
    fs.unlinkSync(SYNC_REQUEST_PATH);
  } catch (_) {
    return; // another instance already consumed it
  }

  const config = cfg || DEFAULTS;
  const langs =
    Array.isArray(req.langs) && req.langs.length > 0
      ? req.langs
      : Object.keys(config.remoteSources);

  if (langs.length === 0) {
    logInfo(
      "[sync] Request received but no langs to sync —",
      "check remoteSources in .i18n-viewer.json",
    );
    return;
  }

  logInfo("[sync] Processing sync request for:", langs.join(", "));

  try {
    const results = await syncTranslations(langs);
    const ok = results.filter((r) => r.status === "ok");
    const failed = results.filter((r) => r.status === "error");
    const skipped = results.filter((r) => r.status === "skip");

    const parts = [];
    if (ok.length > 0)
      parts.push(`synced: ${ok.map((r) => `${r.lang}(${r.keys})`).join(" ")}`);
    if (failed.length > 0)
      parts.push(
        `failed: ${failed.map((r) => `${r.lang} — ${r.error}`).join("; ")}`,
      );
    if (skipped.length > 0)
      parts.push(
        `skipped (no remote): ${skipped.map((r) => r.lang).join(" ")}`,
      );

    logInfo("[sync] Done.", parts.join(" | "));
  } catch (e) {
    logError("[sync] Unexpected error:", e.message);
  }
}, 500).unref(); // .unref() so the timer won't prevent a clean process exit
