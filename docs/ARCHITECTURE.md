# Architecture

## Components

| Component | Tech | Source | Responsibility |
|-----------|------|--------|----------------|
| **Zed extension** | Rust → WASM component (`wasm32-wasip2`) | This repo `src/lib.rs` | Start `httpyac-lsp`; optionally set `HTTPYAC_BIN` |
| **httpyac-lsp** | Native Rust binary | **Custom** (`lsp/`) | Completions (HTTP + **httpyac script/meta**), document symbols, optional runnable metadata |
| **httpyac-run** | Native Rust binary | **Custom** | ▶ entry: resolve `activeEnv` / switch env, then call `httpyac` |
| **httpyac** | Node CLI | **Official** `npm i -g httpyac` | **All runtime**: env, `{{var}}`, scripts, HTTP, response |
| **tasks.json** | Zed tasks | `languages/http/tasks.json` | Gutter ▶ menu (Send / Switch Environment / …) |
| **grammars/http.wasm** | tree-sitter | Third-party grammar build | Syntax highlighting for `.http` |

> **The language server is custom-built for this extension.**  
> It is **not** an official httpyac product. Official httpyac executes requests; this repo only integrates with Zed.

## Send pipeline (engine = httpyac only)

```
User clicks ▶ Send Request
        │
        ▼
  languages/http/tasks.json
  command = httpyac-run
  args    = $ZED_FILE  $ZED_ROW
        │
        ▼
  httpyac-run (Rust)
  · Read nearest http-client.env.json → activeEnv
  · Build: httpyac send <file> --line <N> [--env <name>]
  · cwd = directory of the .http file
        │
        ▼
  httpyac CLI (official)
  · Load environment
  · Resolve {{base_url}} etc.
  · Run scripts
  · Send HTTP and print the response
```

**This repository never sends HTTP itself** (no reqwest execution path for Send).

## Switch environment (no send)

```
🌐 Switch Environment
  → httpyac-run <file> --switch-env
  → Interactive pick: dev / prod / …
  → Only updates "activeEnv" in http-client.env.json
  → No extra files, no httpyac send
```

Later **▶ Send Request** uses the updated `activeEnv`.

## Environment files

| File | Purpose |
|------|---------|
| `http-client.env.json` | Public envs + `activeEnv` (safe to commit) |
| `http-client.private.env.json` | Secret overrides (should be gitignored) |

Search scope: directory of the `.http` file and all parent directories.

## Binary resolution

**httpyac-lsp**

1. `worktree.which("httpyac-lsp")`
2. `$HOME/.local/bin/httpyac-lsp`

**httpyac-run** (from tasks)

1. `httpyac-run` on `PATH` (installed to `~/.local/bin`)
2. Internally resolves `HTTPYAC_BIN` / `which httpyac` / common paths

**httpyac**

1. `HTTPYAC_BIN`
2. `which` / `where`
3. Portable candidates under `$HOME`, `/usr/local`, Homebrew, nvm, etc.

## Why WASM?

Zed loads extensions as a **WASM** module (`extension.wasm`).  
Implementing `zed::Extension` (to start the LSP) requires a `wasm32-wasip2` **component** build (not MVP wasip1 modules).  
`httpyac-lsp` and `httpyac-run` are **native** binaries, not WASM.

## Autocomplete layers (important)

There are **two different levels** of “autocomplete”:

### 1. httpyac-lsp (this extension — HTTP / httpyac domain)

Implemented in `lsp/src/main.rs` (`completion`) + `lsp/src/completions.rs`.

| Context | Suggestions |
|---------|-------------|
| Request line | HTTP methods |
| Headers | Names + common values / auth schemes |
| `{{` | File `@var`, env vars, builtins (`$uuid`, `$timestamp`, …) |
| `# @…` / `// @…` | Meta: `@name`, `@ref`, `@import`, `@loop`, … |
| Script / `> {% %}` / `{{ … }}` | `response` / `request` / `client` / `console` and properties |
| After request | Snippets for response handlers |

**Runtime** of scripts remains **httpyac CLI only**.

### 2. JS inside scripts (highlighting + httpyac-lsp — not full tsserver)

Full completion for **Node APIs** such as:

```js
const crypto = require('crypto');
crypto.createHmac(...)  // ← needs a real JS/TS language service on this buffer
```

is **not** available inside `.http` today. Zed attaches language servers to the
**buffer language** (`HTTP` → `httpyac-lsp` only). Tree-sitter injections do
**not** start `vtsls` / `typescript-language-server` on each script island.

| Piece | Role |
|-------|------|
| **Tree-sitter injection** (`injections.scm`) | JS/JSON **highlighting** in `> {% %}` / `< {% %}` / bodies |
| **httpyac-lsp** | httpyac APIs + curated Node (`crypto.` / `fs.` / `.update`/`.digest`) |
| **vtsls / tsserver** | Only on real `.js`/`.ts` buffers — **not** injection islands in `.http` |
| **httpyac CLI** | Actually executes the script |

Practical split:

| Goal | Where |
|------|--------|
| `Host`, `{{base_url}}`, `client.global.set` | **httpyac-lsp** |
| `require('crypto')`, `crypto.createHmac`, `.update`/`.digest` | **httpyac-lsp** (curated Node surface) |
| Full TS types / every prototype member | **Not in `.http`** (Zed: no range-scoped multi-LSP yet) |
| Actually run the script | **httpyac CLI** |

### Why not wire real JS LSP onto HTTP?

Zed attaches language servers to the **buffer language**. Injections highlight embedded JS but do not start `vtsls` on those ranges. Listing `vtsls` under `languages.HTTP` would treat the whole `.http` file as TypeScript (noise on `GET`/headers). Vue/Svelte solve this with one multi-language server, not injection+tsserver. Until Zed supports multi-LSP documents, curated **httpyac-lsp** members are the practical path.

## Related docs

- [SETUP.md](./SETUP.md) — install and paths  
- [CONTRIBUTING.md](./CONTRIBUTING.md) — contribution rules  
- [../README.md](../README.md) — user guide  
