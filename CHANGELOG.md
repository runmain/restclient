# Changelog

## [0.1.2] - 2026-08-02

### Fixed

- Resolve `httpyac-lsp` like official extensions: settings binary → PATH → bundled `lsp/` → `~/.local/bin`
- Register language server with `languages = ["HTTP"]` (Zed docs form)
- Ship **snippets** (`snippets/http.json`) so **Host** / headers / methods complete even if LSP is slow
- Recommend `completions.words = "disabled"` for HTTP (buffer words like Hello never helped Host)

## [0.1.1] - 2026-08-02

### Fixed

- **Critical:** build `extension.wasm` as a **WASM component** (`wasm32-wasip2`)  
  Previously used `wasm32-wasip1` (MVP module). Zed failed to load the extension with  
  `attempted to parse a wasm module with a component parser`, so **httpyac-lsp never started**  
  and completions only showed buffer words (e.g. `Hello` instead of `Host`).
- Declare `language_servers = ["httpyac-lsp"]` in HTTP `config.toml` and settings examples.

## [0.1.0] - 2026-08-02

### Added

- Zed extension using the official **httpyac CLI** as the only request engine  
- **Custom** `httpyac-lsp` (Rust): completions, document symbols  
- **Custom** `httpyac-run` (Rust): Send entry point and environment switching  
- Task menu: Send / **Switch Environment** / **Send with Environment…** / New Tab  
- Environments: only `activeEnv` in `http-client.env.json` (no extra files)  
- `install_to_zed.sh` for macOS + Linux, comment-safe settings handling  
- Docs: README, SETUP, ARCHITECTURE, CONTRIBUTING, logo  

### Notes

- No custom HTTP client for Send  
- Settings with `//` comments are not auto-rewritten unless `FORCE_ZED_SETTINGS=1`  
