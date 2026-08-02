# Setup guide

## 1. Install the httpyac CLI (required)

```bash
npm install -g httpyac
httpyac --version
```

Without httpyac, the extension can install but cannot execute requests.

## 2. Build tools

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
rustup target add wasm32-wasip2
```

Optional: `python3` (used by the install script when merging settings).

## 3. Build and install the extension

```bash
cd httpyacclient
./install_to_zed.sh
```

On **macOS** and **Linux** the script will:

1. Build **httpyac-lsp** and **httpyac-run**
2. Build the WASM extension → `extension.wasm`
3. Install into Zed’s extensions directory
4. Copy binaries to `~/.local/bin/`
5. Detect paths; handle `settings.json` in a **comment-safe** way

### Install locations

| Artifact | macOS | Linux |
|----------|-------|-------|
| Extension | `~/Library/Application Support/Zed/extensions/installed/httpyacclient` | `~/.config/zed/extensions/installed/httpyacclient` |
| Settings | **`~/.config/zed/settings.json`** (preferred; Application Support only as fallback) | `~/.config/zed/settings.json` |
| httpyac-lsp | `~/.local/bin/httpyac-lsp` | Same |
| httpyac-run | `~/.local/bin/httpyac-run` | Same |

### PATH (required for Send)

```bash
export PATH="$HOME/.local/bin:$PATH"
# Add to ~/.zshrc or ~/.bashrc, then source
which httpyac-run httpyac-lsp
```

Zed’s ▶ buttons invoke **`httpyac-run`**, which must be on `PATH`.

### Install script environment variables

| Variable | Meaning |
|----------|---------|
| `SKIP_ZED_SETTINGS=1` | Do not modify settings.json |
| `FORCE_ZED_SETTINGS=1` | Force-merge settings (**drops `//` comments**; timestamped backup) |
| `NO_COLOR=1` | Disable ANSI colors |

## 4. Restart Zed

**Fully quit** the app (macOS: `Cmd+Q`, not only close the window), then reopen.

## 5. Smoke test

1. Open `examples/basic.http` → ▶ **Send Request**
2. Open `examples/environments.http` (with `http-client.env.json` nearby)
   - **Switch Environment** → only updates `activeEnv`
   - **Send Request** → should use current env (no `base_url is not defined`)
   - **Send with Environment…** → pick env in the terminal, then send

CLI equivalents:

```bash
cd examples
httpyac-run environments.http 9              # use activeEnv
httpyac-run environments.http --switch-env   # switch only
httpyac-run environments.http 9 --env prod   # fixed env
httpyac-run environments.http 9 --pick-env   # pick then send
```

## Optional `settings.json`

The extension works without writing settings if PATH is correct. To pin absolute paths:

```json
{
  "httpyac": {
    "command": "/path/to/httpyac",
    "lsp_command": "/path/to/httpyac-lsp",
    "default_env": "dev"
  },
  "lsp": {
    "httpyac-lsp": {
      "binary": {
        "path": "/path/to/httpyac-lsp",
        "arguments": []
      }
    }
  },
  "languages": {
    "HTTP": {
      "language_servers": ["httpyac-lsp"],
      "completions": {
        "words": "fallback",
        "words_min_length": 3
      }
    }
  }
}
```

If settings contain `//` comments, the install script **skips auto-write** by default and prints the snippet to merge manually.

## Embedded JS in scripts (what works / what does not)

| Layer | What you get |
|-------|----------------|
| **httpyac-lsp** | `response.` / `request.` / `client.` / `console.` / `crypto.` / `require('…')` (curated) |
| **JSON/XML/GraphQL body injection** | Body **highlighting** only |
| **JS injection into scripts** | **Disabled on purpose** — if enabled, Zed routes completions to **vtsls**, which errors on `.http` and **hides** httpyac-lsp results |
| **vtsls on `languages.HTTP`** | **Do not enable.** Logs show: `Get completion via vtsls failed: Reduce of empty array…` |

### Required settings for script completions

```json
"languages": {
  "HTTP": {
    "language_servers": ["httpyac-lsp"],
    "completions": { "lsp": true, "words": "disabled" }
  }
}
```

**Never** add `"vtsls"` (or `typescript-language-server`) next to `httpyac-lsp` for HTTP.

Runtime of scripts is always **httpyac CLI**.

## Troubleshooting

| Symptom | Action |
|---------|--------|
| No ▶ buttons | Fully restart Zed; confirm extension under `installed/` |
| `httpyac-run: command not found` | Put `~/.local/bin` on PATH; restart Zed |
| `base_url is not defined` | Use **httpyac-run** (with `--env`); put `@vars` **before** first `###`; check `activeEnv` |
| No Switch Environment menu | Reinstall / refresh `tasks.json`; restart Zed |
| LSP not running | `ls ~/.local/bin/httpyac-lsp`; Server Logs → HTTPyac LSP |
| No full `crypto.` in `.http` | Move logic to `examples/scripts/*.js` and use vtsls there ([VTSLS-SCRIPTS.md](./VTSLS-SCRIPTS.md)), or add APIs under `.script/*.js` ([SCRIPT-EXT.md](./SCRIPT-EXT.md)) |
| Custom `request.headers.set` / own helpers | Add any `.js` under `.script/` (nested exports + `@httpyac-path`) — [SCRIPT-EXT.md](./SCRIPT-EXT.md) |
| `const c = require('crypto'); c.` empty | Rebuild/reinstall httpyac-lsp; binding + `.script/crypto.js` merge is supported |
| vtsls errors on `.http` | Remove `vtsls` from `languages.HTTP.language_servers` |

See also [README.md](../README.md).
