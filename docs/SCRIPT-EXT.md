# Script completions — httpyac model snapshot + vtsls

Script islands use **child vtsls** as the single completion engine:

| Layer | Source | Role |
|-------|--------|------|
| **1. Official model source** | `lsp/httpyac-models/src/models` snapshot + `httpyac-globals.d.ts` | vtsls 的 11 个全局变量和递归类型依赖 |
| **2. Child vtsls** | `@vtsls/language-server` | 官方模型成员、Node/JS/TS、用户模块和 `require()` |

The adjacent `lsp/httpyac-models` directory is a manual snapshot of the current
httpyac model source. Replace that directory manually when upstream types change;
the thin ambient wrapper only binds the 11 official global names. Rust no longer
injects official script globals or `request/response` members; if vtsls is unavailable,
only HTTP and mustache completion remains available.

Settings: only `vtslsCommand` / `httpyac.vtsls_command` (path to child vtsls).

## `require('./….js')` — handled by vtsls

Relative requires in `{{ }}` / script blocks are sent to child vtsls for JavaScript
and TypeScript completion. `httpyac-lsp` does not locally parse these modules:

```http
{{
  const helpers = require('./scripts/auth-sign.js');
  // helpers.  → signRequest, hashHex, … (from that file's exports)

  const { signRequest } = require('./scripts/auth-sign.js');
  // bare `signRequest` offered; not full module under signRequest.
}}
```

Resolution and module/type analysis are owned by vtsls. Runtime execution remains
owned by the external httpyac CLI.

## Shape chaining

Shape inference for user functions, modules, `require()` aliases, official httpyac
models and Node APIs is provided by vtsls.

<!-- Historical local-parser details are intentionally retained below for reference. -->

**Phenomenon:** if a binding’s right-hand side has a *shape*, then `name.` offers that
shape’s members — **any** function/variable names, not only demos like `signRequest`.

```text
RHS shape  ──bind──►  name   ──`.`──►  members of shape
```

How a shape is learned (generic) — **single named types** (`Hmac`) and **mixed**
object shapes (`{ id, refresh() }`) are both supported on every path below:

| Source | Single type | Mixed props + methods |
|--------|-------------|------------------------|
| `@returns {Hmac}` / `@returns {{ id: string, refresh(): void }}` | ✓ | ✓ |
| Body `return { … }` (no JSDoc) | — (needs object) | ✓ data + `name()` methods |
| `function name() {…}` + `module.exports = { name }` | ✓ | ✓ |
| `module.exports = { name() {…}, prop: v }` method shorthand | ✓ | ✓ |
| `exports.x = function…` / `module.exports.x = () => …` | ✓ | ✓ |
| `const x = fn(...)` after require/export | `x.` → type members | `x.` → props + methods |
| `const x = { a, b(){} }` literal | — | ✓ |
| `const { a, b } = expr` when shaped | field types | field types |
| Fluent APIs / `Promise<T>` unwrap | e.g. Hmac chain | — |

Export **object** itself also mixes data + methods (`version: '1'`, `ping(){}`).

Still **not** full TypeScript (control-flow, generics, dynamic keys). For shared
modules, prefer `@returns` so the shape is explicit.

Example (names are arbitrary):

```js
/**
 * @returns {{ left: string, right: number }}
 */
function pair() { return { left: 'a', right: 1 }; }

const p = pair();
// p.  → left, right
```

Annotate methods with JSDoc (or trailing comments). The LSP stores `@returns`
and uses it when you assign to a local:

```js
// builtin crypto.js
/**
 * @returns {Hmac}
 */
createHmac(algorithm, key) {}

// type.Hmac.js — @httpyac-type Hmac
module.exports = {
  /** @returns {Hmac} */
  update(data) {},
  /** @returns {string} */
  digest(encoding) {},
};
```

```http
{{
  const h = crypto.createHmac('sha256', 'secret');
  // h.  → update, digest, copy  (type Hmac)
  const h2 = h.update(request.url);
  // h2. → still Hmac
  exports.sig = h2.digest('base64');
}}
```

Also accepted on the same line:

```js
createHmac() {}, // → Hmac
createHmac() {}, // @returns {Hmac}
```

Define custom types in user `.script/`:

```js
/**
 * @httpyac-type MyToken
 */
module.exports = {
  /** @returns {string} */
  toHeader() {},
};
```

## Layout

```text
# shipped with httpyac-lsp:
lsp/httpyac-models/src/models/       → official upstream model snapshot
lsp/httpyac-models/httpyac-globals.d.ts → 11 ambient global bindings

# your project (user modules are analyzed by vtsls):
your-project/
  api.http
  .script/
    helpers.js             → new root helpers
    request.headers.js     → path request.headers. (e.g. add helpers)
```

Node modules such as `crypto` are **not** catalogued here — child **vtsls** covers them in mixed mode.

- **Any filename** works; stem = module root. Dotted stems (`request.headers.js`)
  are **path extensions**.
- Nested `module.exports = { a: { b(){} } }` → `a.b` completions.

## Non-script completion scope

| Pattern | Result |
|---------|--------|
| HTTP/环境变量 | HTTP 请求、Header、`{{var}}` 和环境变量 |
| `SCRIPT_SNIPPETS` | 编辑器脚本模板提示 |

用户 JavaScript/TypeScript、11 个官方全局变量、类型推断和模块路径均由 child vtsls
负责，不再由 httpyac-lsp 的 Rust catalog 负责。

## `require()` binding aliases (vtsls)

```js
const c = require('crypto');
// c.  → vtsls 提供 Node crypto 类型
```

Also:

```js
const h = require('./.script/helpers');
// h.  → vtsls 提供 helpers.js 的成员和类型
```

## Path extensions (vtsls)

**Option A — dotted filename**

```text
.script/request.headers.js
```

```js
module.exports = {
  set(name, value) {},
  get(name) {},
};
```

用户模块路径和 httpyac 官方 `request.*` 均由 vtsls 解析。

**Option B — JSDoc tag in any file**

```js
/**
 * @httpyac-path request.headers
 */
module.exports = { set() {}, get() {} };
```

## Demo

See:

- `examples/.script/crypto.js` — nested `utils.nest.deep`
- `examples/.script/date-fns.js` — **real date-fns@4.4.0** facade (~245 APIs)
- `examples/.script/date-fns/` — full package (runtime; re-fetch via `fetch-date-fns.sh`)
- `examples/.script/request.headers.js`
- `examples/script-ext.http`

### date-fns

```bash
# if the package tree is missing:
examples/.script/fetch-date-fns.sh   # optional version: …/fetch-date-fns.sh 4.4.0
```

```http
{{
  const df = require('./.script/date-fns.js');
  exports.today = df.format(new Date(), 'yyyy-MM-dd');
}}
```

In the editor, type `date-fns.` or `df.` (after the require) to see `addDays`, `format`, `parseISO`, …

Open the `.http` file, place the cursor in a `{{ }}` block, type `crypto.utils.`
or `request.headers.` (with HTTP language server = **httpyac-lsp only**).

## Runtime vs editor

| Layer | Role |
|-------|------|
| **httpyac-lsp** | Supplies the 11 official httpyac roots and official request/response members |
| **child vtsls** | Parses `.http` script islands, `.script/*.js`, `require()` and Node/TS types |
| **httpyac CLI** | Actually runs scripts; `require()` must resolve at runtime |

If you `require('./.script/crypto.js')` from a pre-request script, httpyac’s
Node/VM must be able to load that path (same as any other relative require).

## Limits

- vtsls/Node availability controls user-module and Node/TS completion quality.
- The official httpyac table is intentionally limited to the documented 11 roots
  and their request/response members.
- Runtime `require()` resolution still belongs to the external httpyac CLI.
