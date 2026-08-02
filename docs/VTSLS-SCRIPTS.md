# Full Node / vtsls IntelliSense with httpyac

## Goal

| Want | Where it works |
|------|----------------|
| Complete Node API on a **real** `.js` / `.ts` buffer | **Zed-managed vtsls** (open the file) |
| Complete Node/TS **inside** `.http` `{{ }}` / handlers | **httpyac-lsp child vtsls** (`vtsls --stdio`, configured path) |
| Send request / `{{vars}}` / env / HTTP lines | **httpyac-lsp** only (never put Zed `vtsls` on `languages.HTTP`) |

Zed attaches language servers to the **buffer language**. Listing editor `vtsls` under HTTP still breaks `GET` lines.

**Script-region path (this branch):** install `@vtsls/language-server`, set `httpyac.vtsls_command` /
`lsp.httpyac-lsp.settings.vtslsCommand`. httpyac-lsp spawns that binary, feeds a virtual `.ts`
document for script islands, and forwards completion / hover / definition. This is a **private**
vtsls process (not Zed’s language-server UI entry) — see [VTSLS-MULTI-LSP-REVIEW.md](./VTSLS-MULTI-LSP-REVIEW.md).

## Recommended layout

```text
project/
  examples/
    script-vtsls.http          # thin require + exports
    scripts/
      jsconfig.json            # vtsls / checkJs for this folder
      auth-sign.js             # full crypto / business logic  ← open this for IntelliSense
  http-client.env.json
```

### 1. Heavy logic in JS (true vtsls)

```js
// examples/scripts/auth-sign.js
const crypto = require('crypto');

function signRequest(request, secret = 'secret') {
  const date = new Date();
  const signatureBase64 = crypto
    .createHmac('sha256', secret)   // ← vtsls: full members
    .update(`${request.method}…`)
    .digest('base64');
  return { authDate: date.toUTCString(), authentication: `Basic ${signatureBase64}` };
}

module.exports = { signRequest };
```

Open **`auth-sign.js`** in Zed → language JavaScript/TypeScript → **vtsls** → `crypto.` / any binding works.

### 2. Thin glue in `.http` (httpyac runtime)

```http
{{
  const { signRequest } = require('./scripts/auth-sign.js');
  const signed = signRequest(request, 'secret');
  exports.authDate = signed.authDate;
  exports.authentication = signed.authentication;
}}
POST https://httpbin.org/anything
Date: {{authDate}}
Authentication: {{authentication}}
```

httpyac CLI `require()`s the file at **send** time. Zed does not need vtsls on the `.http` buffer for that.

### 3. Settings (required)

```json
{
  "languages": {
    "HTTP": {
      "language_servers": ["httpyac-lsp"],
      "completions": { "lsp": true, "words": "disabled" }
    },
    "JavaScript": {
      "language_servers": ["vtsls", "..."]
    }
  }
}
```

- **HTTP** → only `httpyac-lsp`  
- **JavaScript** → `vtsls` (default / built-in)  
- **Never** `"HTTP": { "language_servers": ["httpyac-lsp", "vtsls"] }`

Optional: Node types for better JS checking

```bash
npm i -D @types/node
```

(with `examples/scripts/jsconfig.json` already setting `"types": ["node"]`).

## What stays in `.http` vs `.js`

| Keep in `.http` | Move to `.js` |
|-----------------|---------------|
| One-liners, `exports.x = …` | HMAC / crypto / Buffer pipelines |
| `client.test` / small asserts | Large test helpers |
| `@name` / env / headers | Shared auth, signing, parsing |
| `require('./scripts/…')` glue | Anything you want full IntelliSense on |

## Mental model

```text
┌─────────────────────────────┐     require()      ┌──────────────────────────┐
│  foo.http  (language HTTP)  │ ─────────────────► │  scripts/*.js (language  │
│  httpyac-lsp: Host, {{var}} │   at httpyac run   │  JS + vtsls: crypto.… )  │
│  thin {{ require + export }}│                    │                          │
└─────────────────────────────┘                    └──────────────────────────┘
```

## Demo

1. Open `examples/scripts/auth-sign.js` → type `crypto.` → full vtsls list through `.digest`.  
2. Open `examples/script-vtsls.http` → ▶ Send → httpyac loads the module.  
3. Confirm HTTP language servers UI shows **httpyac-lsp** only (not vtsls).

## Future

If Zed gains range-scoped multi-LSP for injections, we could re-enable JS injection + vtsls on script islands. Until then, **external modules** are the supported way to get “true vtsls” with httpyac.
