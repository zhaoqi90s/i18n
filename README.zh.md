# i18n Key Translator

[Zed 编辑器](https://zed.dev) 扩展，在编辑器中提供 **i18n 键悬停预览**功能。

将鼠标悬停在源码中的任意 i18n 键上，即可即时看到对应的翻译文案——支持从远程 URL 获取资源并缓存到本地，以富文本 Markdown 气泡的形式展示。

同时在 AI Assistant 面板中提供 `/i18n`、`/i18n-keys` 和 `/i18n-sync` 三个斜杠命令。

---

## 功能特性

| 功能 | 说明 |
|---|---|
| **悬停预览** | 鼠标悬停在 `t("some.key")` 上，即可内联查看翻译文案 |
| **远程资源** | 按语言配置远程 URL，自动下载并缓存翻译文件 |
| **本地缓存** | 缓存文件在重启后依然有效，支持配置缓存有效期（TTL） |
| **多语言展示** | 在单个悬停卡片中同时展示多种语言的翻译 |
| **正则匹配** | 支持任意调用方式：`t(...)`, `formatMessage(...)`, `$t(...)` 等 |
| **键前缀** | 源码使用短键时自动补全命名空间前缀，再进行翻译查找 |
| **本地兜底** | 未配置远程资源时，自动回退到本地 JSON 文件 |
| **斜杠命令** | 在 Assistant 面板中使用 `/i18n`、`/i18n-keys`、`/i18n-sync` |

---

## 环境要求

- [Zed](https://zed.dev)（任意近期稳定版本）
- **Node.js** — 用于运行悬停预览的 LSP 服务。Zed 会自动管理其内置的 Node.js，大多数情况下无需额外安装。

---

## 安装方式

### 从 Zed 扩展市场安装

1. 打开 Zed。
2. 按 `Cmd+Shift+X`（macOS）或 `Ctrl+Shift+X`（Linux）打开扩展面板。
3. 搜索 **i18n Key Translator**，点击 **Install** 安装。

### 以本地开发扩展方式安装

```sh
git clone https://github.com/yourusername/i18n-key-translator
```

然后在 Zed 中：**Extensions → Install Dev Extension**，选择克隆下来的目录即可。

---

## 配置

在**项目根目录**（与 `package.json`、`Cargo.toml` 等同级）创建 `.i18n-viewer.json` 文件。

> 可以将扩展自带的 `.i18n-viewer.json.example` 复制到项目根目录后重命名，作为配置起点。

```json
{
  "defaultLang": "zh",
  "languages": ["zh", "en", "ja"],
  "remoteSources": {
    "zh": "https://cdn.example.com/locales/zh.json",
    "en": "https://cdn.example.com/locales/en.json",
    "ja": "https://cdn.example.com/locales/ja.json"
  },
  "localPaths": [
    "locales/{lang}.json",
    "public/locales/{lang}/translation.json"
  ],
  "keyPrefix": "",
  "patterns": [
    "formatMessage\\([\"']([^\"']+)[\"']",
    "\\$t\\([\"']([^\"']+)[\"']",
    "\\bt\\([\"']([^\"']+)[\"']",
    "i18n\\.t\\([\"']([^\"']+)[\"']"
  ],
  "cacheDir": ".i18n-cache",
  "ttl": 3600
}
```

### 配置项说明

| 字段 | 类型 | 默认值 | 说明 |
|---|---|---|---|
| `defaultLang` | `string` | `"en"` | 悬停卡片中显示的主语言，斜杠命令的默认语言 |
| `languages` | `string[]` | `[]` | 悬停卡片中额外展示的语言列表 |
| `remoteSources` | `{ [lang]: url }` | `{}` | 语言代码 → 远程 JSON URL 的映射 |
| `localPaths` | `string[]` | *（内置列表）* | 相对于项目根目录的路径模板，`{lang}` 会在运行时替换。非空时**完全替换**内置列表。 |
| `keyPrefix` | `string` | `""` | 查找前自动追加到每个提取键的前缀。适用于翻译文件使用完整命名空间键、而源码只引用短键的场景。 |
| `patterns` | `string[]` | *（内置列表）* | JavaScript 正则字符串，**第 1 个捕获组**必须捕获 i18n 键。非空时完全替换内置列表。 |
| `cacheDir` | `string` | `".i18n-cache"` | 缓存目录，相对于项目根目录，用于存储下载的翻译文件 |
| `ttl` | `number` | `3600` | 缓存有效期（秒），超时后重新拉取远程资源 |

### 键前缀（keyPrefix）

`keyPrefix` 适用于翻译文件使用带命名空间的完整键、而源码只引用短键的场景：

```json
// zh.json（扁平结构，完整键名）
{
  "app.button.save": "保存",
  "app.button.cancel": "取消"
}
```

```ts
// 源码中使用短键
t("button.save")
```

配置 `"keyPrefix": "app."` 后，扩展会自动查找 `app.button.save`。查找按以下顺序依次尝试，命中即停止：

1. `data["prefix + key"]` — 扁平格式 + 前缀（推荐）
2. `data.prefix.key` — 嵌套点分隔 + 前缀（兜底）
3. `data["key"]` — 扁平格式，无前缀（兜底）
4. `data.key` — 嵌套点分隔，无前缀（兜底）

---

## 悬停预览

配置完成后，将鼠标悬停在任意被匹配到的 i18n 键上：

```ts
// 悬停在 "auth.errors.invalidEmail" 上，将显示：
//
//   i18n key: `auth.errors.invalidEmail`
//
//   [zh]  请输入有效的电子邮件地址。
//   [en]  Please enter a valid email address.
//   [ja]  有効なメールアドレスを入力してください。

const msg = t("auth.errors.invalidEmail");
```

悬停卡片由扩展内置的轻量级 Node.js LSP 服务生成，通过标准 LSP 协议（stdin/stdout）与 Zed 通信。

### 支持的文件类型

JavaScript、TypeScript、JSX、TSX、Vue、Svelte、Python、Rust、Go、Ruby、PHP、Java、Kotlin、Swift、C#。

如需支持其他语言，欢迎提 Issue 或 PR——只需在 `extension.toml` 中添加对应的语言条目即可。

---

## 翻译文件格式

远程和本地资源均需为 **JSON** 格式（支持扁平或嵌套结构）：

```json
{
  "common": {
    "button": {
      "save": "保存",
      "cancel": "取消"
    }
  },
  "auth": {
    "errors": {
      "invalidEmail": "请输入有效的电子邮件地址。"
    }
  }
}
```

键的访问使用**点分隔路径**：`common.button.save`。

---

## 匹配模式（Patterns）

`patterns` 数组接受 JavaScript 兼容的正则字符串，**第 1 个捕获组**必须捕获 i18n 键。

在配置中提供 `patterns` 数组后，将**完全替换**内置列表。

### 内置模式

| 模式 | 匹配示例 |
|---|---|
| `formatMessage\(["']([^"']+)["']` | `formatMessage("key")` |
| `\$t\(["']([^"']+)["']` | `$t('key')`（Vue） |
| `\bt\(["']([^"']+)["']` | `t('key')`（i18next、react-i18next） |
| `i18n\.t\(["']([^"']+)["']` | `i18n.t('key')` |
| `i18n\(["']([^"']+)["']` | `i18n('key')` |
| `gettext\(["']([^"']+)["']` | `gettext('key')` |
| `translate\(["']([^"']+)["']` | `translate('key')` |
| `intl\.formatMessage\(\{[^}]*id:\s*["']([^"']+)["']` | `intl.formatMessage({ id: 'key' })` |
| `<Trans[^>]+i18nKey=["']([^"']+)["']` | `<Trans i18nKey="key" />`（React） |

---

## 远程资源缓存机制

1. 首次悬停时，从配置的 URL 下载对应语言的翻译文件。
2. 下载结果保存至项目内 `<cacheDir>/<lang>.json`。
3. 后续悬停直接读取磁盘缓存，直到 `ttl` 秒后过期。
4. 缓存过期后，后台静默重新拉取。

> **提示：** 建议将 `.i18n-cache/` 加入项目的 `.gitignore`，避免将缓存文件提交到版本库。

---

## 斜杠命令

扩展在 **AI Assistant 面板**中注册了三个斜杠命令。

### `/i18n <key> [lang]`

查询并展示某个翻译键的值。

```
/i18n common.button.save
/i18n auth.errors.invalidEmail zh
```

### `/i18n-keys [lang]`

列出指定语言翻译文件中所有的叶子键，按字母排序。

```
/i18n-keys
/i18n-keys ja
```

### `/i18n-sync [lang]`

展示所有已配置语言的缓存状态，并触发远程资源下载。不指定语言时，同步所有已配置的远程源。

```
/i18n-sync
/i18n-sync zh
```

> **注意：** `/i18n-sync` 需要至少打开一个受支持的源文件（如 `.ts`、`.vue` 等）以启动 LSP 服务。若服务尚未启动，请先打开一个源文件，再重新执行命令。

三个命令均遵循相同的 `.i18n-viewer.json` 配置和本地文件查找逻辑。

---

## 本地路径模板

当 `localPaths` 为空（或未配置）时，扩展会按顺序自动搜索以下路径：

```
locales/{lang}.json
locale/{lang}.json
i18n/{lang}.json
translations/{lang}.json
lang/{lang}.json
public/locales/{lang}/translation.json
public/locales/{lang}/common.json
public/locales/{lang}/index.json
locales/{lang}/translation.json
locales/{lang}/index.json
locales/{lang}/common.json
locale/{lang}/translation.json
src/locales/{lang}.json
src/i18n/{lang}.json
src/i18n/locales/{lang}.json
src/assets/locales/{lang}.json
assets/locales/{lang}.json
assets/i18n/{lang}.json
lib/l10n/app_{lang}.arb
```

---

## 架构说明

```
┌─────────────────────────────────┐
│  Zed 编辑器                      │
│  ┌──────────────────────────┐   │
│  │ 扩展（WASM）              │   │   斜杠命令
│  │  src/lib.rs              │   │   /i18n  /i18n-keys  /i18n-sync
│  │  src/config.rs           │   │
│  │  src/translation.rs      │   │
│  │  src/commands/           │   │
│  └──────────┬───────────────┘   │
│             │ language_server_command
│             ▼                   │
│  ┌──────────────────────────┐   │
│  │ LSP 服务（Node.js）       │   │   悬停预览
│  │  lsp/server.js           │   │   textDocument/hover
│  │                          │   │
│  │  • 读取 .i18n-viewer     │   │
│  │  • 拉取远程资源           │   │
│  │  • 读取本地文件           │   │
│  │  • 内存 + 磁盘双层缓存    │   │
│  │  • 轮询 .sync-request    │   │
│  └──────────────────────────┘   │
└─────────────────────────────────┘
```

- **WASM 扩展** 负责处理斜杠命令并启动 LSP 服务。由于 WASM 沙箱无法直接发起 HTTP 请求，远程资源的拉取通过写入 `.sync-request` IPC 文件委托给 LSP 服务执行。
- **Node.js LSP 服务** 处理 `textDocument/hover` 请求，通过 stdin/stdout 与 Zed 通信。该服务**无任何 npm 依赖**，仅使用 Node.js 内置模块（`http`、`https`、`fs`、`path`）。

---

## 开发指南

### 前置条件

| 工具 | 说明 |
|------|------|
| **Rust** | 通过 [rustup](https://rustup.rs) 安装，**不要**用 Homebrew |
| **wasm32-wasip2 编译目标** | `rustup target add wasm32-wasip2` |
| **Node.js** | 仅用于语法检查；扩展运行时由 Zed 内置的 Node.js 负责 |

### 项目结构

```
src/
├── lib.rs           # 扩展入口 — Extension trait 实现、LSP 启动
├── langs.rs         # COMMON_LANGS，用于斜杠命令参数补全
├── config.rs        # Config 结构体、DEFAULT_PATH_TEMPLATES、load_config()
├── translation.rs   # 文件查找、JSON 格式化、键查找、键枚举
└── commands/
    ├── mod.rs
    ├── i18n.rs      # /i18n 命令
    ├── i18n_keys.rs # /i18n-keys 命令
    └── i18n_sync.rs # /i18n-sync 命令
lsp/
└── server.js        # Node.js LSP 服务 — 处理 textDocument/hover
```

> **注意：** `server.js` 在编译时通过 `include_str!` 嵌入到 WASM 二进制中。
> 因此，无论是修改 Rust 代码还是 `lsp/server.js`，都需要重新编译 WASM 才能生效。

### 首次初始化

```sh
git clone https://github.com/yourusername/i18n-key-translator
cd i18n-key-translator

# 添加 WASM 编译目标（如尚未安装）
rustup target add wasm32-wasip2

# 在 Zed 中注册为开发扩展：
# Extensions → Install Dev Extension → 选择本目录
```

### 开发循环

```sh
# 1. 快速检查编译错误（不输出 WASM，速度很快）
cargo check --target wasm32-wasip2

# 2. 全量构建前先验证 server.js 语法
node --check lsp/server.js

# 3. 编译 WASM
cargo build --target wasm32-wasip2

# 4. 在 Zed 中热重载扩展（无需重启）：
#    命令面板 → "zed: reload extensions"
```

### 查看日志

```sh
# 方式 A — 命令面板（Zed 运行时）
# 命令面板 → "zed: open log"

# 方式 B — 将日志直接打印到终端
zed --foreground
```

### 手动测试 LSP 服务

```sh
node lsp/server.js
# 在 stdin 输入 LSP JSON-RPC 消息，从 stdout 查看响应。
# 示例 initialize 请求：
# {"jsonrpc":"2.0","id":1,"method":"initialize","params":{"capabilities":{}}}
```

---

## 常见问题

### 斜杠命令提示"No translation file found"（未找到翻译文件）

1. 确认打开的是**项目文件夹**，而非单个文件。
2. 不带参数执行 `/i18n-keys`，报错信息中会列出所有尝试过的路径。
3. 在项目根目录创建 `.i18n-viewer.json`，配置适合你项目的 `localPaths` 或 `remoteSources`。

### 悬停时没有任何反应

1. 检查 `.i18n-viewer.json` 中的 `patterns` 是否能匹配你的代码调用方式。
2. 在命令面板执行 `Zed: Open Log` 查看 LSP 服务的日志输出（日志写入 stderr）。
3. 确认 Zed 已成功下载并管理其内置 Node.js（首次启动时会自动完成）。

### `/i18n-sync` 提示 LSP 服务未启动

打开任意受支持的源文件（`.ts`、`.js`、`.vue` 等）以激活语言服务器，再重新执行 `/i18n-sync`。

### 翻译文件解析报错

扩展仅支持标准 JSON，请确认翻译文件：
- 没有多余的尾随逗号
- 使用 UTF-8 编码保存
- 不是 YAML、TOML 或 JSON5 格式

### WASM 构建失败

- 确认 Rust 是通过 `rustup` 安装的，而非 Homebrew。
- 若提示缺少编译目标，运行 `rustup target add wasm32-wasip2`。
- 使用 `zed --foreground` 启动 Zed，扩展日志会直接打印到终端。

---

## 参与贡献

欢迎提交 Pull Request！较大的改动请先开 Issue 讨论。

---

## 许可证

MIT — 详见 [LICENSE](./LICENSE)。