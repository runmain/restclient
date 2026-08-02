; Language injections for HTTP / httpyac (tree-sitter-http).
;
; What this gives in Zed today:
;   ✅ Syntax highlighting for embedded JS / JSON / XML / GraphQL
;   ❌ Full JS/TS language-server IntelliSense (crypto., Buffer, …) is NOT
;      attached to injected regions in a .http buffer — Zed runs LSPs on the
;      buffer language (HTTP → httpyac-lsp), not on each injection island.
;
; Domain APIs (response. / client. / request.) come from httpyac-lsp.
; Node names match rest-nvim/tree-sitter-http.

; ── Script blocks: > {% … %}  and  < {% … %} ──────────────────────────
; Prefer the inner `script` node (content between {% and %}).
; Fallback: whole handler / pre-request node if structure differs.

((script) @injection.content
 (#set! injection.language "javascript")
 (#set! injection.include-children))

((res_handler_script (script) @injection.content)
 (#set! injection.language "javascript")
 (#set! injection.include-children))

((pre_request_script (script) @injection.content)
 (#set! injection.language "javascript")
 (#set! injection.include-children))

; If grammar exposes handler without nested script capture, still try whole node
((res_handler_script) @injection.content
 (#set! injection.language "javascript")
 (#set! injection.include-children))

((pre_request_script) @injection.content
 (#set! injection.language "javascript")
 (#set! injection.include-children))

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
