# Review: script-completion / builtin `.script` work (2026-08)

## What was reviewed

Uncommitted/recent work: builtin `lsp/builtin_script/*`, user `.script/` merge,
return-type chaining, `script_ext` catalog, completion path in `main.rs`, docs.

## Findings

| Issue | Severity | Disposition |
|-------|----------|-------------|
| `Position.character` treated as UTF-8 byte index → mid-char slice panic → Zed `server shut down` | **Critical** | **Fixed**: `lsp_utf16_to_byte` / `byte_to_lsp_utf16` |
| User `.script/crypto.js` override wiped `@returns {Hmac}` → `sha256.` chain dead | **High** | **Fixed earlier**: preserve returns on merge + method-name fallbacks |
| Bare `update`/`copy` defaulted to type `Hmac` (poisons Hash/Sign) | Medium | **Fixed**: only constructors + `digest` use name fallbacks |
| Unbounded user `.script` (e.g. vendored package dumps) can hang/OOM catalog load | Medium | **Fixed**: max 64 files, 512KiB each; top-level only |
| Catalog load panic could abort completion task | Medium | **Fixed**: per-file `catch_unwind`; user load catch; catalog_for_uri fallback to builtin |
| Hardcoded `REQUEST_PROPS` tables vs JS builtins | Low | **Kept**: tables unused for members; JS is source of truth (simpler to extend) |
| `default_returns` is not full typing | Low | **Kept on purpose**: heuristic only; not vtsls |
| HTTP + vtsls in settings | Doc | **Kept documented**: SETUP/SCRIPT-EXT say never pair them |
| Working-tree loss of `examples/scripts/auth-sign.js` + `jsconfig.json` (still in HEAD; required by `script-vtsls.http` / VTSLS-SCRIPTS) | **High** (regression) | **Restored** from HEAD — two-buffer vtsls demo intact |
| Empty root `test.js` left untracked | Low | **Deleted** (accidental leftover) |

## Kept on purpose

- No real vtsls inside `.http` script islands (Zed buffer-language limit).
- **Two-buffer vtsls demo** remains: `examples/scripts/auth-sign.js` + `examples/script-vtsls.http` (full Node tips in real `.js`; thin `require` in `.http`).
- Builtin APIs as parseable JS (same path as user extensions).
- User same-name override of builtins (with return-type preservation).
- Curated crypto fluent types (`type.Hmac.js`, …), not full Node prototypes.

## Settings / docs

- `languages.HTTP.language_servers`: **only** `httpyac-lsp` (SETUP + README).
- Do not enable JS injection on scripts or vtsls on HTTP.
- SCRIPT-EXT documents `@returns` / `@httpyac-type` chaining.

## Crash posture after this review

1. UTF-16 cursor → safe byte boundary (unit-tested incl. CJK/emoji).
2. Hostile/partial `.script` JS: parse failures isolated; completion still serves builtins.
3. Oversized `.script` files skipped rather than loaded whole.
