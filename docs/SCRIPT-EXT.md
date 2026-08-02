# `.script/` — builtin + user JS modules for completions

httpyac-lsp loads **two layers** of JS modules (same parse rules):

| Layer | Location | Role |
|-------|----------|------|
| **1. Builtin** | `lsp/builtin_script/*.js` (embedded in the binary) | `request` / `response` / `client` / `console` / `crypto` / … |
| **2. User** | project `.script/*.js` (next to `.http` or any parent) | extras + **overrides** |

**User wins** on the same module path and member name.

## `require('./….js')` — load into the completion engine

Relative requires in `{{ }}` / script blocks are **parsed for tips** (editor-only):

```http
{{
  const helpers = require('./scripts/auth-sign.js');
  // helpers.  → signRequest, hashHex, … (from that file's exports)

  const { signRequest } = require('./scripts/auth-sign.js');
  // bare `signRequest` offered; not full module under signRequest.
}}
```

Resolution is relative to the **`.http` file’s directory**. Files larger than 512KiB are skipped. Builtin / `.script/` catalogs still apply; required files merge on top by stem name.

## Shape chaining (general rule, not special cases)

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
# shipped with httpyac-lsp (not in your repo):
lsp/builtin_script/
  request.js
  response.js      # includes response.headers.get / valueOf
  client.js        # includes client.global.set / get / …
  console.js
  crypto.js
  …

# your project:
your-project/
  api.http
  .script/
    crypto.js              → overrides/extends builtin crypto.
    helpers.js             → new root helpers.
    request.headers.js     → path request.headers. (e.g. add .set)
```

- **Any filename** works; stem = module root. Dotted stems (`request.headers.js`)
  are **path extensions**.
- Nested `module.exports = { a: { b(){} } }` → `a.b` completions.

## What is parsed

| Pattern | Result |
|---------|--------|
| `function name() {}` | `name` |
| `const name = () => {}` | `name` |
| `exports.name = …` / `module.exports.name = …` | `name` |
| `module.exports = { a, b: { c(){} } }` | `a`, `b`, `b.c` |
| `class X { method() {} }` | `method` (and related) |
| `/** @httpyac-path request.headers */` | attach file exports to that path |

Comments are stripped for structure parsing; `@httpyac-path` is read from the
original source.

## `require()` binding aliases

```js
const c = require('crypto');
// c.  → same member table as crypto. (builtin node:crypto + .script/crypto.js)
```

Also:

```js
const h = require('./.script/helpers');
// h.  → members parsed from helpers.js
```

## Path extensions (e.g. `request.headers.set`)

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

Typing `request.headers.` suggests `set` / `get`.

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
| **httpyac-lsp** | Parses `.script/*.js` for **completions only** |
| **httpyac CLI** | Actually runs scripts; `require()` must resolve at runtime |

If you `require('./.script/crypto.js')` from a pre-request script, httpyac’s
Node/VM must be able to load that path (same as any other relative require).

## Limits

- Heuristic parser (not full ES/TS AST): unusual syntax may be missed.
- Not a substitute for **vtsls** on real `.js` buffers (see [VTSLS-SCRIPTS.md](./VTSLS-SCRIPTS.md)).
- Re-reads `.script/` when file mtimes change (cached per LSP process).
