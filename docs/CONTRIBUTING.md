# Contributing

Thanks for contributing to **HTTPyac Client for Zed**.

## Prerequisites

- Rust stable + `wasm32-wasip2`: `rustup target add wasm32-wasip2`
- [httpyac](https://httpyac.github.io/): `npm install -g httpyac`
- [Zed](https://zed.dev/)
- Recommended: `python3` (settings merge in the install script)

## Build and install

```bash
cd lsp
cargo test
cargo build --release
cd ..

cargo build --release --target wasm32-wasip2
./install_to_zed.sh
```

Fully quit and restart Zed after install.

## Design rules

1. **All request execution goes through the official httpyac CLI.**  
   Do not reintroduce reqwest / a custom HTTP client for Send.
2. **httpyac-lsp is a custom LSP** for the editor only (completions, symbols, lines). It does not send HTTP.
3. **httpyac-run** only assembles args / env, then runs httpyac.
4. **Env switch only updates `activeEnv` in `http-client.env.json`** — no extra sidecar files.
5. **No machine-specific absolute paths** in source; use `which`, `$HOME`, `$XDG_CONFIG_HOME`.
6. **`install_to_zed.sh` must work on macOS and Linux**, and **`install_to_zed.ps1` must install on Windows**; `codesign` only on macOS.
7. Keep install UX clear: highlight steps the user must do manually.

## Layout

```
httpyacclient/
├── src/lib.rs                 # WASM: start httpyac-lsp
├── lsp/
│   ├── src/lib.rs             # Shared: env discovery, path resolution
│   ├── src/main.rs            # httpyac-lsp
│   ├── src/bin/httpyac_run.rs # Send / switch-env entry
│   ├── src/parser.rs
│   ├── src/variables.rs
│   └── src/completions.rs
├── languages/http/            # Language, highlights, tasks.json
├── grammars/http.wasm
├── examples/
├── docs/
├── extension.toml
├── install_to_zed.sh
└── install_to_zed.ps1
```

## Style

```bash
cd lsp
cargo fmt
cargo clippy -- -D warnings
cargo test
```

## Documentation

When changing behavior, update:

- [README.md](../README.md)
- [SETUP.md](./SETUP.md)
- [ARCHITECTURE.md](./ARCHITECTURE.md)
- [CHANGELOG.md](../CHANGELOG.md)

## Pull requests

- Note impact on Send / env behavior  
- Smoke-test with `./install_to_zed.sh` on macOS or Linux  
- Do not commit `target/`, `extension.wasm`, or private env files  
