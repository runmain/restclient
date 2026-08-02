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
| **Tree-sitter injection** (`injections.scm`) | JS/JSON **syntax highlighting** inside `> {% … %}` / `< {% … %}` |
| **httpyac-lsp** | Completions for `response.` / `request.` / `client.` / `console.` / `JSON.` / common JS keywords & `require('…')` modules |
| **vtsls / typescript-language-server** | **Not attached** to `.http` buffers. Zed runs language servers for the **file language** (HTTP), not for each injected island. So you will **not** get full Node IntelliSense like `crypto.createHmac` method lists inside scripts. |

Runtime of scripts is always **httpyac CLI** (its JS VM), independent of the editor LSP.

To edit complex JS with full IntelliSense, keep it in a separate `.js` file and `@import` / call out from httpyac if your workflow allows — or rely on httpyac-lsp’s script hints above.
## Troubleshooting

| Symptom | Action |
|---------|--------|
| No ▶ buttons | Fully restart Zed; confirm extension under `installed/` |
| `httpyac-run: command not found` | Put `~/.local/bin` on PATH; restart Zed |
| `base_url is not defined` | Use **httpyac-run** (with `--env`); put `@vars` **before** first `###`; check `activeEnv` |
| No Switch Environment menu | Reinstall / refresh `tasks.json`; restart Zed |
| LSP not running | `ls ~/.local/bin/httpyac-lsp`; Server Logs → HTTPyac LSP |
| No `crypto.` methods | Install/enable JS/TS in Zed (editor-level); httpyac-lsp only covers httpyac APIs |

See also [README.md](../README.md).
