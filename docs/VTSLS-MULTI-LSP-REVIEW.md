# Review: Zed multi-LSP + vtsls inside `.http` scripts

Branch: **`vtsls`**.  
Source: Google AI Mode Q&A about “HTTP 片段用 httpyac-lsp、script 用 vtsls”.

This note compares that answer with **what this repo already knows and ships**.

---

## Short verdict

| AI claim | Reality in Zed + this project |
|----------|-------------------------------|
| Zed can list multiple LSPs per language | **True** (`language_servers: ["a","b"]`) |
| That gives range-split (GET→http LSP, `{{script}}`→vtsls) | **False** — both see the **whole buffer** |
| Tree-sitter injection alone starts vtsls on islands | **False** — injection ≈ highlight; LSP still bound to buffer language |
| Pair `httpyac-lsp` + `vtsls` on HTTP | **Harmful** — vtsls errors on `GET`/headers; we already saw empty-completion / noise |
| httpyac-lsp must be a Zed extension | **True** — we already are (`extension.toml` + WASI host) |
| Best “true vtsls in script” long-term = proxy inside httpyac-lsp | **Architecturally sound**, **not implemented**, non-trivial |
| Two-buffer (`.js` + thin `.http`) | **Supported today** — see [VTSLS-SCRIPTS.md](./VTSLS-SCRIPTS.md) |
| Curated / `.script` tips inside `.http` | **Supported today** — see [SCRIPT-EXT.md](./SCRIPT-EXT.md) |

**Do not** put `"vtsls"` next to `"httpyac-lsp"` under `languages.HTTP`.

---

## What Zed actually does

```text
.http buffer language = HTTP
        │
        ▼
 languages.HTTP.language_servers  →  [httpyac-lsp]   (and optionally others)
        │
        ▼
 every completion/hover/diagnostic request carries the FULL document URI + position
        │
        ▼
 each listed LSP gets the same full .http text — no automatic “only this injection range”
```

Implications:

1. **Multi-LSP ≠ multi-region.** Angular/Tailwind multi-LSP is “several servers on one language,” not “this range only.”
2. **Injections** (`injections.scm`) improve **highlighting**. They do **not** attach vtsls to `{{ }}` islands. We **deliberately do not** inject `javascript` into scripts for that reason (see `languages/http/injections.scm` comments).
3. Adding vtsls on HTTP makes vtsls parse `GET https://…` as TypeScript → diagnostics spam / broken completions (documented in SETUP).

Zed still lacks stable **embedded / range-scoped multi-LSP** (community issues exist; not a product we can rely on yet).

---

## Three strategies (ranked for *this* repo)

### A. Two-buffer (shipped — recommended for “true vtsls”)

```text
auth-sign.js  ──vtsls──►  full Node IntelliSense
     ▲
     │ require()
foo.http      ──httpyac-lsp──►  Host / {{var}} / thin glue
```

- Docs: [VTSLS-SCRIPTS.md](./VTSLS-SCRIPTS.md)  
- Demo: `examples/script-vtsls.http` + `examples/scripts/auth-sign.js`  
- Zero Zed multi-LSP risk; runtime is still httpyac CLI.

### B. Curated catalog inside `.http` (shipped — good enough for daily tips)

- Builtin `lsp/builtin_script/*` + user `.script/*.js`  
- `@returns` single + mixed shapes, require aliases, call-result members  
- Docs: [SCRIPT-EXT.md](./SCRIPT-EXT.md)  
- **Not** full TypeScript / every Node prototype — by design.

### C. httpyac-lsp as vtsls **proxy** (AI “ultimate” path — design only)

```text
Zed  ──all .http LSP──►  httpyac-lsp  ──script ranges only──►  vtsls --stdio
                              │
                              └── GET/headers/{{var}} handled locally
```

**Why the AI is right that this can work:**

- One LSP id for Zed → no dual-server merge bugs on HTTP.  
- Virtual doc (`*.http` → synthetic `.ts` / padded lines) keeps positions mappable.  
- vtsls never sees bare `GET` lines → no red-squiggle storm.

**Why we have not done it (and should treat as a project, not a weekend):**

| Hard part | Detail |
|-----------|--------|
| Process | Spawn/manage `vtsls` (or tsserver) lifecycle, crash restart, multi-workspace |
| Protocol | Full client: `initialize`, `didOpen`/`didChange`, request id correlation, cancellation |
| Mapping | Multi-script islands, offsets, UTF-16 positions (we already hardened our own) |
| Semantics | httpyac globals (`request`, `response`, `client`, `exports`) are **not** Node — need stubs/`jsconfig` ambient for vtsls |
| WASI host | Extension host is WASI; **LSP binary is native** (ok), but packaging/docs for “user must have vtsls on PATH” |
| UX | Latency, when to forward, merge with our own catalog tips |
| Fallback | vtsls missing → degrade to catalog (current behavior) |

Rough phases if we implement on this branch later:

1. **Detect** cursor in script / handler range (parser already knows blocks).  
2. **Virtual buffer** sync: extract scripts → one or N virtual `.ts` docs.  
3. **JSON-RPC client** to child `vtsls --stdio` (tower client or hand-rolled framing).  
4. **Forward** completion/hover/definition only; keep diagnostics optional/filtered.  
5. **Ambient** `request`/`response`/`client` typings so vtsls is useful, not angry.  
6. **Settings** flag: `lsp.httpyac-lsp.settings.vtslsProxy: true` default off until solid.

Until then, **A + B** remain the product answer.

---

## Fact-check of the AI write-up

### Correct enough

- Multi language servers per language exist in Zed settings.  
- Whole-file vtsls on `.http` will mis-parse HTTP syntax.  
- Extension registration is required for custom LSP (we already have `httpyac-lsp`).  
- Proxy/forwarding is how many multi-language tools survived before editor-native islands.  
- Range mapping + virtual URI are the hard core of a proxy.

### Wrong or misleading

| AI said | Correction |
|---------|------------|
| “写几行 Rust 扩展才能用 httpyac-lsp” | **Already done** — this repo *is* that extension. |
| Injection → automatic JS completions from vtsls | **No** on Zed today for LSP routing the way described. |
| Settings multi-LSP is a practical “方案二” for this use case | **We forbid it** for HTTP; empirically breaks tips. |
| “cmd-, opens settings” as the main fix | Config is necessary for **A**, not a fix for island LSP. |
| Proxy is “best and straightforward in Rust” | Best *if* you need in-buffer full TS; **cost is high**. |

### Already better than the AI’s “方案二”

Our docs and `injections.scm` encode the failed experiment:

- No JS injection on scripts.  
- HTTP language servers: **only** `httpyac-lsp`.  
- True vtsls → real `.js` buffers.  
- In-buffer tips → `script_ext` catalog.

---

## Decision for branch `vtsls`

| Option | Action on this branch |
|--------|------------------------|
| **Stay with A+B** | Document only (this file); no code. Default product. |
| **Spike proxy** | Prototype child vtsls + one completion path behind a flag; measure latency and ambient types. |
| **Wait for Zed islands** | Watch embedded multi-LSP issues; re-enable JS injection only if editor routes by range. |

**Recommendation:** keep shipping **A + B**. Treat **C** as optional R&D on `vtsls` if someone needs full TS *inside* `{{ }}` without opening a second file — not a prerequisite for daily httpyac use.

---

## Related

- [VTSLS-SCRIPTS.md](./VTSLS-SCRIPTS.md) — two-buffer workflow  
- [SCRIPT-EXT.md](./SCRIPT-EXT.md) — in-`.http` catalog  
- [SETUP.md](./SETUP.md) — “never add vtsls on HTTP”  
- [ARCHITECTURE.md](./ARCHITECTURE.md) — buffer-language model  
- `languages/http/injections.scm` — why scripts are not JS-injected  
- `examples/script-vtsls.http` — live demo of A  
