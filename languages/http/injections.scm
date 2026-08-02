; Language injections for HTTP / httpyac (tree-sitter-http).
;
; IMPORTANT — do NOT inject `javascript` into script / handler regions.
; When JS is injected, Zed routes completions to vtsls (if present). vtsls then
; fails on .http buffers ("Reduce of empty array…") and httpyac-lsp crypto./
; response./client. completions never appear — even though httpyac-lsp works.
; Script IntelliSense is provided by httpyac-lsp instead.
;
; Bodies still inject JSON / GraphQL / XML for highlighting only.

; ── Bodies ────────────────────────────────────────────────────────────
((json_body) @injection.content
 (#set! injection.language "json")
 (#set! injection.include-children))

((graphql_body) @injection.content
 (#set! injection.language "graphql")
 (#set! injection.include-children))

((graphql_data) @injection.content
 (#set! injection.language "graphql")
 (#set! injection.include-children))

((xml_body) @injection.content
 (#set! injection.language "xml")
 (#set! injection.include-children))
