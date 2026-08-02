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
| Proxy `completion` to a **child** `vtsls --stdio` | **Not** “using Zed’s vtsls” — only **hands off some** methods (see below) |
| Two-buffer (`.js` + thin `.http`) | **Supported today** — this *is* full Zed-managed vtsls on real JS |
| Curated / `.script` tips inside `.http` | **Supported today** — see [SCRIPT-EXT.md](./SCRIPT-EXT.md) |

**Do not** put `"vtsls"` next to `"httpyac-lsp"` under `languages.HTTP`.

---

## Critical distinction: Zed’s vtsls vs proxy hand-off

The AI snippet looks like “script uses vtsls”:

```rust
if self.is_inside_script_block(...) {
    vtsls_params...uri = self.get_virtual_ts_uri(uri);
    let vtsls_res = self.vtsls_client.call("textDocument/completion", ...).await?;
    return Ok(Some(corrected_res));
}
```

That is **not** integrating **Zed’s** vtsls. It is:

| | **Zed-managed vtsls** | **httpyac-lsp child proxy** |
|--|----------------------|------------------------------|
| Who starts the process | **Zed** (extension / built-in TS stack) | **httpyac-lsp** (`Command::new("vtsls")`) |
| Who owns lifecycle / restart | Zed | We do |
| Settings | `lsp.vtsls` in Zed settings | Separate; easy to diverge |
| Workspace / multi-file | Real project buffers Zed already opened | Virtual URI + only what we `didOpen` |
| Methods | Full LSP surface Zed wires (complete, hover, def, refs, rename, code action, semantic tokens, …) | **Only what we forward** — snippet usually only `completion` |
| Buffer language | Real **JavaScript/TypeScript** | Still **HTTP**; Zed never talks to vtsls for this file |
| Diagnostics / UI | First-class in editor | Optional, must re-map and re-publish ourselves |

So the proxy is accurately described as:

> **把部分能力交出去** — hand a *subset* of IntelliSense to a *private* vtsls instance, then translate ranges back.

It is **not**:

> 完整使用 Zed 里的 vtsls（编辑器级、全协议、与 JS 项目同一套配置）.

### What “部分” usually means in practice

Forwarded (if we invest):

- `textDocument/completion` (and maybe `completionItem/resolve`)
- sometimes `hover`, `definition`

Usually **not** free with the toy snippet:

- project-wide references / rename  
- code actions, organize imports  
- semantic tokens, inlay hints  
- signature help parity  
- diagnostics as Zed would show for a real `.ts` tab  
- Zed’s vtsls init options, plugins, yarn PNP, etc.  
- httpyac globals (`request` / `response` / `client` / `exports`) without our ambient stubs  

Even a “complete” proxy is still a **second** language service, not the same process Zed uses for `auth-sign.js`.

### What *is* full Zed vtsls today

**Strategy A (two-buffer)** only:

```text
Open examples/scripts/auth-sign.js
  → buffer language JavaScript
  → Zed starts/attaches its real vtsls
  → full protocol, full settings, real node_modules
```

That is the only path in this repo that **完整使用 Zed 的 vtsls**.  
`.http` keeps httpyac-lsp (Host / vars / thin `require`).

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

### C. httpyac-lsp **child proxy** (partial hand-off — design only, not “Zed vtsls”)

```text
Zed  ──only talks to──►  httpyac-lsp  ──optional subset──►  private vtsls child
         (HTTP buffer)         │              (completion/… only)
                               └── GET / headers / {{var}} / catalog locally
```

**Correct framing:** export/delegate *some* script IntelliSense to an embedded
language service. **Incorrect framing:** “script 块用上了 Zed 的 vtsls”.

**Why it can still be useful (limited):**

- Zed only sees one HTTP LSP → no dual-server merge on `GET`.  
- Child never parses raw HTTP lines → no TS errors on request lines.  
- Virtual doc + range map can make **completions** feel “smarter than catalog”.

**Why it is incomplete by construction:**

- Not Zed’s process, settings, or buffer association.  
- Only methods we implement (toy code = completion only).  
- Virtual files ≠ real multi-file TS project as the user edits `.js` tabs.  
- Always a maintenance tax next to catalog (B) and two-buffer (A).

**Engineering cost** (if ever spiked): process lifecycle, full client protocol,
multi-island map, UTF-16, ambient httpyac types, PATH to `vtsls`, merge/fallback
with catalog, feature flag default off.

Until Zed has **range-scoped multi-LSP** that attaches **its** vtsls to
injections, **A** remains the only “完整 Zed vtsls”; **B** remains in-buffer tips;
**C** is optional R&D for partial in-`{{ }}` TS-like completion — not a substitute
for opening a real `.js` file.

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
| Proxy is “best and straightforward in Rust” | Cost is high; and it is **only partial hand-off**, not full Zed vtsls. |
| Snippet `call("textDocument/completion")` = 用上 vtsls | = **交出去一部分** completion；hover/def/project 仍缺除非继续堆转发. |

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

**Recommendation:**

- Want **完整 Zed-managed vtsls** → open real `.js` (A).  
- Want tips **inside** `.http` without vtsls installed → catalog (B).  
- Want **full TS completion/hover/definition inside script islands** → **C is implemented**
  on branch `vtsls`: install `@vtsls/language-server`, set `httpyac.vtsls_command` /
  `lsp.httpyac-lsp.settings.vtslsCommand` (see `install_to_zed.sh`). Still a **child**
  process hand-off, not Zed’s language-server row for the HTTP buffer.

---

## Related

- [VTSLS-SCRIPTS.md](./VTSLS-SCRIPTS.md) — two-buffer workflow  
- [SCRIPT-EXT.md](./SCRIPT-EXT.md) — in-`.http` catalog  
- [SETUP.md](./SETUP.md) — “never add vtsls on HTTP”  
- [ARCHITECTURE.md](./ARCHITECTURE.md) — buffer-language model  
- `languages/http/injections.scm` — why scripts are not JS-injected  
- `examples/script-vtsls.http` — live demo of A  
