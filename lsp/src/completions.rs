//! Completion vocabularies for HTTP + httpyac syntax (editor-only; execution is httpyac CLI).

pub const HTTP_METHODS: &[(&str, &str)] = &[
    ("GET", "Retrieve data"),
    ("POST", "Submit data"),
    ("PUT", "Update/replace data"),
    ("PATCH", "Partial update"),
    ("DELETE", "Remove data"),
    ("HEAD", "Get headers only"),
    ("OPTIONS", "Get allowed methods"),
    ("CONNECT", "Establish tunnel"),
    ("TRACE", "Loop-back test"),
];

/// Common HTTP request headers (label, default/example value).
/// Includes Host and other frequently used names for httpyac / REST clients.
pub const HEADER_NAMES: &[(&str, &str)] = &[
    // Core
    ("Host", "example.com"),
    ("Content-Type", "application/json"),
    ("Content-Length", ""),
    ("Content-Encoding", "gzip"),
    ("Content-Language", "en-US"),
    ("Accept", "application/json"),
    ("Accept-Encoding", "gzip, deflate, br"),
    ("Accept-Language", "en-US,en;q=0.9"),
    ("Accept-Charset", "utf-8"),
    ("Authorization", "Bearer "),
    ("User-Agent", "httpyac"),
    ("Cookie", ""),
    ("Connection", "keep-alive"),
    ("Cache-Control", "no-cache"),
    ("Pragma", "no-cache"),
    ("Origin", "https://example.com"),
    ("Referer", "https://example.com/"),
    ("Referrer-Policy", "no-referrer"),
    // Conditional / cache
    ("If-None-Match", ""),
    ("If-Modified-Since", ""),
    ("If-Match", ""),
    ("If-Unmodified-Since", ""),
    ("If-Range", ""),
    ("ETag", ""),
    ("Age", ""),
    ("Expires", ""),
    ("Last-Modified", ""),
    ("Vary", "Accept-Encoding"),
    // Content negotiation / transfer
    ("Transfer-Encoding", "chunked"),
    ("TE", "trailers"),
    ("Trailer", ""),
    ("Range", "bytes=0-"),
    ("Expect", "100-continue"),
    ("Upgrade", "websocket"),
    ("Via", ""),
    // Security / CORS (often used in tests)
    ("X-Content-Type-Options", "nosniff"),
    ("X-Frame-Options", "DENY"),
    ("X-XSS-Protection", "1; mode=block"),
    ("Strict-Transport-Security", "max-age=31536000"),
    ("Content-Security-Policy", "default-src 'self'"),
    ("Access-Control-Allow-Origin", "*"),
    (
        "Access-Control-Allow-Methods",
        "GET, POST, PUT, DELETE, OPTIONS",
    ),
    (
        "Access-Control-Allow-Headers",
        "Content-Type, Authorization",
    ),
    ("Access-Control-Allow-Credentials", "true"),
    ("Access-Control-Request-Method", "POST"),
    ("Access-Control-Request-Headers", "Content-Type"),
    // Proxy / forwarding
    ("Forwarded", ""),
    ("X-Forwarded-For", ""),
    ("X-Forwarded-Host", ""),
    ("X-Forwarded-Proto", "https"),
    ("X-Real-IP", ""),
    ("Proxy-Authorization", "Basic "),
    ("Proxy-Connection", "keep-alive"),
    // API / custom common
    ("X-Request-ID", ""),
    ("X-Request-Id", ""),
    ("X-Correlation-ID", ""),
    ("X-API-Key", ""),
    ("X-Api-Key", ""),
    ("X-CSRF-Token", ""),
    ("X-Requested-With", "XMLHttpRequest"),
    // Auth extras
    ("WWW-Authenticate", "Bearer"),
    ("Proxy-Authenticate", ""),
    // Misc
    ("Date", ""),
    ("Location", ""),
    ("Server", ""),
    ("Allow", "GET, POST, OPTIONS"),
    ("Link", ""),
    ("Retry-After", ""),
    ("Warning", ""),
    ("Max-Forwards", ""),
    ("From", ""),
    ("DNT", "1"),
    ("Sec-Fetch-Dest", "empty"),
    ("Sec-Fetch-Mode", "cors"),
    ("Sec-Fetch-Site", "same-origin"),
    ("Sec-CH-UA", ""),
    ("Sec-CH-UA-Mobile", "?0"),
    ("Sec-CH-UA-Platform", ""),
];

pub const AUTH_SCHEMES: &[&str] = &["Bearer ", "Basic ", "Token "];

/// httpyac / IntelliJ-style meta directives (usually on `# @…` or `// @…` lines).
pub const META_DIRECTIVES: &[(&str, &str)] = &[
    ("@name", "Name this request (reference later)"),
    ("@ref", "Reference another named request"),
    ("@import", "Import another .http file"),
    ("@loop", "Loop over a list or count"),
    ("@disabled", "Skip this request"),
    ("@no-redirect", "Do not follow redirects"),
    ("@no-cookie-jar", "Disable cookie jar"),
    ("@timeout", "Request timeout (ms)"),
    ("@note", "Note / description"),
];

/// Built-in dynamic variables: {{$uuid}}, {{$timestamp}}, …
pub const BUILTIN_VARS: &[(&str, &str)] = &[
    ("$uuid", "Random UUID"),
    ("$timestamp", "Unix timestamp (ms)"),
    ("$isoTimestamp", "ISO-8601 timestamp"),
    ("$randomInt", "Random integer (use: $randomInt min max)"),
    ("$processEnv", "Process env (use: $processEnv NAME)"),
    ("$dotenv", "Value from .env (use: $dotenv KEY)"),
    ("$guid", "Alias for UUID"),
];

/// Official httpyac script globals on the VM global object.
/// Source: https://httpyac.github.io/guide/scripting.html — “Access to Variables”.
/// Node/JS APIs (`crypto`, `JSON`, keywords, …) are left to child vtsls, not listed here.
///
/// Tuple: `(name, description, condition)` — condition empty means always available.
pub const HTTPYAC_OFFICIAL_GLOBALS: &[(&str, &str, &str)] = &[
    (
        "$global",
        "Object which allows storing global Variables",
        "",
    ),
    (
        "$requestClient",
        "requestClient to send additional body in streaming event",
        "",
    ),
    ("httpFile", "current httpFile", ""),
    ("httpRegion", "current httpRegion", ""),
    (
        "oauth2Session",
        "OAuth2 Response",
        "only if OAuth2/ OpenId Connect is used",
    ),
    ("request", "request of the next http request", ""),
    (
        "response",
        "http response of the last executed request",
        "only use it in post request scripts or for responses imported with @forceRef",
    ),
    (
        "sleep",
        "Method to wait for a fixed period of time",
        "",
    ),
    (
        "test",
        "method to simplify tests (assert or chai)",
        "",
    ),
    (
        "__dirname",
        "path to current working directory",
        "",
    ),
    (
        "__filename",
        "The file name of the current module",
        "",
    ),
];

/// Bare roots offered in script islands (official globals only).
pub const SCRIPT_ROOTS: &[(&str, &str)] = &[
    ("$global", "Object which allows storing global Variables"),
    (
        "$requestClient",
        "requestClient to send additional body in streaming event",
    ),
    ("httpFile", "current httpFile"),
    ("httpRegion", "current httpRegion"),
    (
        "oauth2Session",
        "OAuth2 Response (only if OAuth2/ OpenId Connect is used)",
    ),
    ("request", "request of the next http request"),
    (
        "response",
        "http response of the last executed request (post-request / @forceRef)",
    ),
    ("sleep", "Method to wait for a fixed period of time"),
    ("test", "method to simplify tests (assert or chai)"),
    ("__dirname", "path to current working directory"),
    ("__filename", "The file name of the current module"),
];

/// Completions after `response.` — httpyac `HttpResponse` fields only
/// (`dist/models/httpResponse.d.ts`). No IntelliJ aliases (`status`, `responseTime`).
pub const RESPONSE_PROPS: &[(&str, &str)] = &[
    ("protocol", "Protocol (HTTP, …)"),
    ("name", "Response / request name if set"),
    ("httpVersion", "HTTP version string"),
    ("statusCode", "HTTP status code (number)"),
    ("statusMessage", "Reason phrase"),
    ("headers", "Response headers (Record; use bracket access)"),
    ("contentType", "Parsed content-type"),
    ("body", "Body (string / object depending on content)"),
    ("parsedBody", "Parsed JSON/XML body when available"),
    ("prettyPrintBody", "Pretty-printed body string"),
    ("rawHeaders", "Raw header lines"),
    ("rawBody", "Raw body Buffer"),
    ("request", "The request object that produced this response"),
    ("timings", "Timing phases (HttpTimings)"),
    ("meta", "Extra metadata map"),
    ("tags", "Tags array"),
];

/// Completions after `response.timings.` — httpyac `HttpTimings`.
pub const RESPONSE_TIMINGS_PROPS: &[(&str, &str)] = &[
    ("wait", "timings.wait"),
    ("dns", "timings.dns"),
    ("tcp", "timings.tcp"),
    ("tls", "timings.tls"),
    ("request", "timings.request"),
    ("firstByte", "timings.firstByte"),
    ("download", "timings.download"),
    ("total", "timings.total"),
];

/// Completions after `request.` — httpyac `Request` / `HttpRequest`
/// (`dist/models/httpRequest.d.ts`).
pub const REQUEST_PROPS: &[(&str, &str)] = &[
    ("url", "Request URL (mutable in pre-request)"),
    ("method", "HTTP method"),
    ("headers", "Request headers (Record; mutable in pre-request)"),
    ("body", "Request body"),
    ("contentType", "Parsed content-type"),
    ("protocol", "Protocol"),
    ("timeout", "Timeout (ms)"),
    ("proxy", "Proxy URL"),
    ("noRedirect", "Disable following redirects"),
    ("noRejectUnauthorized", "Skip TLS verify"),
    ("supportsStreaming", "Streaming support flag"),
    ("options", "Underlying got options (advanced)"),
];

/// Official-native script snippets (no IntelliJ `client.*`).
pub const SCRIPT_SNIPPETS: &[(&str, &str, &str)] = &[
    (
        "script-test",
        "test(…) with assert",
        "test(\"${1:status 200}\", () => {\n  const { equal } = require('assert');\n  equal(response.statusCode, ${2:200});\n});",
    ),
    (
        "script-global",
        "$global.NAME = …",
        "$global.${1:name} = ${2:response.parsedBody};",
    ),
    (
        "script-sleep",
        "await sleep(ms)",
        "await sleep(${1:1000});",
    ),
];

pub fn header_values(name: &str) -> &'static [&'static str] {
    match name {
        "content-type" => &[
            "application/json",
            "application/xml",
            "application/x-www-form-urlencoded",
            "multipart/form-data",
            "text/plain",
            "text/html",
            "text/csv",
            "application/octet-stream",
            "application/graphql",
            "application/jwt",
        ],
        "accept" => &[
            "application/json",
            "application/xml",
            "text/html",
            "text/plain",
            "*/*",
        ],
        "cache-control" => &[
            "no-cache",
            "no-store",
            "max-age=0",
            "max-age=3600",
            "must-revalidate",
            "public",
            "private",
        ],
        "connection" => &["keep-alive", "close"],
        "accept-encoding" => &[
            "gzip, deflate",
            "gzip, deflate, br",
            "gzip",
            "br",
            "identity",
        ],
        "accept-language" => &[
            "en-US",
            "en-US,en;q=0.9",
            "en-GB",
            "zh-CN",
            "ja-JP",
            "*",
        ],
        "content-encoding" => &["gzip", "deflate", "br", "identity"],
        "transfer-encoding" => &["chunked", "compress", "deflate", "gzip"],
        "x-content-type-options" => &["nosniff"],
        "x-frame-options" => &["DENY", "SAMEORIGIN"],
        "access-control-allow-origin" => &["*"],
        "access-control-allow-methods" => {
            &["GET, POST, PUT, DELETE, OPTIONS", "GET, POST, OPTIONS", "*"]
        }
        "access-control-allow-headers" => &["Content-Type, Authorization", "Content-Type", "*"],
        _ => &[],
    }
}

/// True if cursor is inside a httpyac JS script region.
/// Detects: `> {% … %}`, multi-line `{{ … }}` (pre/post request), require/crypto/exports usage.
pub fn in_script_context(lines: &[&str], line_idx: usize, before_cursor: &str) -> bool {
    let line = lines.get(line_idx).copied().unwrap_or("").trim_start();

    // Current line is a handler / script line
    if line.starts_with('>') || line.starts_with('<') && line.contains("{%") {
        return true;
    }
    // Strong JS/httpyac signals on this line (works even if {{ block detect fails)
    let js_signals = [
        "response.",
        "request.",
        "client.",
        "console.",
        "require(",
        "crypto.",
        "exports.",
        "Buffer.",
        "JSON.",
        "Math.",
        "fs.",
        "path.",
        "const ",
        "let ",
        "var ",
        "await ",
        "async ",
        "function ",
        "=>",
    ];
    if js_signals.iter().any(|s| before_cursor.contains(s) || line.contains(s)) {
        return true;
    }

    // Walk up within block for unclosed {% or {{ script
    let block_start = (0..=line_idx)
        .rev()
        .find(|&i| lines[i].trim_start().starts_with("###"))
        .map(|i| i + 1)
        .unwrap_or(0);

    let mut open_brace_script = false;
    let mut open_percent = false;

    for i in block_start..=line_idx {
        let l = lines[i].trim();
        if l.starts_with('>') && l.contains("{%") {
            open_percent = !l.contains("%}");
        }
        if l.starts_with('>') && (l.contains("{{") || l.trim_start_matches('>').trim().starts_with('{'))
        {
            // JetBrains-style or single-line
        }
        // Multi-line {{ script after request (httpyac)
        if l == "{{" || l.starts_with("{{") && !l.contains("}}") {
            open_brace_script = true;
        }
        if open_brace_script && l.contains("}}") {
            open_brace_script = false;
        }
        if open_percent && l.contains("%}") {
            open_percent = false;
        }
        if l.starts_with('>') {
            open_percent = l.contains("{%") && !l.contains("%}");
        }
    }

    // Also: if we're past headers and current/prev lines look like JS in a {{ block
    if open_brace_script || open_percent {
        return true;
    }

    // Line is only inside a multi-line handler started earlier with >
    for i in (block_start..line_idx).rev() {
        let l = lines[i].trim();
        if l.starts_with("###") {
            break;
        }
        if l.starts_with('>') && l.contains("{%") && !l.contains("%}") {
            // find if closed before us
            let closed = ((i + 1)..=line_idx).any(|j| lines[j].contains("%}"));
            if !closed {
                return true;
            }
        }
        if l == "{{" {
            let closed = ((i + 1)..=line_idx).any(|j| lines[j].trim().starts_with("}}") || lines[j].contains("}}"));
            if !closed {
                return true;
            }
        }
        // hit next request line going up — stop
        let first = l.split_whitespace().next().unwrap_or("");
        if matches!(
            first.to_ascii_uppercase().as_str(),
            "GET" | "POST" | "PUT" | "PATCH" | "DELETE" | "HEAD" | "OPTIONS"
        ) && l.split_whitespace().nth(1).is_some()
        {
            break;
        }
    }

    false
}

/// Build Zed-safe member completion fields.
///
/// Zed client-filters with a query that often includes the receiver + dot
/// (`request.`, `response.`, `crypto.`, `client.global.`). Therefore:
/// - `filter_text` / `label` must be **single-line** `path.name` (query is a prefix)
/// - never put bare `name` alone as the only filter when path is non-empty
/// - never use `\n` inside filter_text (breaks Zed fuzzy match)
pub fn member_filter_label(path: &str, name: &str) -> (String, String) {
    if path.is_empty() {
        (name.to_string(), name.to_string())
    } else {
        let full = format!("{path}.{name}");
        (full.clone(), full)
    }
}

/// UTF-16 code unit offset (LSP `Position.character`) → UTF-8 byte index in `line`.
/// Always lands on a char boundary; never panics on out-of-range or mid-code-unit values.
pub fn lsp_utf16_to_byte(line: &str, utf16_col: u32) -> usize {
    if utf16_col == 0 {
        return 0;
    }
    let mut seen = 0u32;
    for (byte_idx, ch) in line.char_indices() {
        if seen >= utf16_col {
            return byte_idx;
        }
        let w = ch.len_utf16() as u32;
        // If utf16_col falls inside a surrogate pair (len_utf16==2), snap to this char start.
        if seen + w > utf16_col {
            return byte_idx;
        }
        seen += w;
    }
    line.len()
}

/// UTF-8 byte index → LSP UTF-16 `Position.character` (clamped to char boundary).
pub fn byte_to_lsp_utf16(line: &str, mut byte_idx: usize) -> u32 {
    if byte_idx > line.len() {
        byte_idx = line.len();
    }
    while byte_idx > 0 && !line.is_char_boundary(byte_idx) {
        byte_idx -= 1;
    }
    line[..byte_idx].encode_utf16().count() as u32
}

/// Byte offset (into `before_cursor` / line prefix) of the member being typed.
pub fn member_replace_start_byte(before_cursor: &str, filter_len: usize) -> usize {
    if before_cursor.ends_with('.') {
        before_cursor.len()
    } else if let Some(dot) = before_cursor.rfind('.') {
        dot + 1
    } else {
        before_cursor.len().saturating_sub(filter_len)
    }
}

/// Legacy helper: member start as LSP UTF-16 column on `line` (prefix = before_cursor).
pub fn member_replace_start(before_cursor: &str, position_character: u32, filter_len: usize) -> u32 {
    let _ = position_character;
    let byte = member_replace_start_byte(before_cursor, filter_len);
    // before_cursor is a prefix of the line; byte index equals line byte index
    byte_to_lsp_utf16(before_cursor, byte)
}

#[cfg(test)]
mod utf16_pos_tests {
    use super::{byte_to_lsp_utf16, lsp_utf16_to_byte};

    #[test]
    fn ascii_roundtrip() {
        let line = "Host: {{api}}";
        for col in 0..=line.len() as u32 {
            let b = lsp_utf16_to_byte(line, col);
            assert!(line.is_char_boundary(b));
            let _ = &line[..b];
            let _ = &line[b..];
        }
    }

    #[test]
    fn chinese_never_panics_mid_char() {
        let line = "Host: {{中文}}";
        // Probe every utf16 column including those that would be mid-scalar if mis-counted
        let u16_len = line.encode_utf16().count() as u32;
        for col in 0..=u16_len + 5 {
            let b = lsp_utf16_to_byte(line, col);
            assert!(line.is_char_boundary(b), "col {col} -> byte {b}");
            let _ = &line[..b];
            let _ = &line[b..];
            let back = byte_to_lsp_utf16(line, b);
            assert!(back <= u16_len + 1);
        }
    }

    #[test]
    fn emoji_boundary() {
        let line = "x👍y"; // thumbs up is one extended grapheme, utf16 len 2
        let b = lsp_utf16_to_byte(line, 2); // after high surrogate only → snap to 👍 start or after
        assert!(line.is_char_boundary(b));
    }
}

/// Dot-path / call-receiver prefix before cursor for property completion.
///
/// - `"response.st"` → `("response", "st")`
/// - `"signed."` → `("signed", "")`
/// - `"signRequest(request, 'secret')."` → `("signRequest(request, 'secret')", "")`
///   (call result members — chain after `)`)
/// - bare `"sign"` → `("", "sign")`
pub fn dotted_prefix(before_cursor: &str) -> Option<(String, String)> {
    let bytes = before_cursor.as_bytes();
    let mut end = bytes.len();
    // skip trailing incomplete ident (filter)
    while end > 0 && (bytes[end - 1].is_ascii_alphanumeric() || bytes[end - 1] == b'_' || bytes[end - 1] == b'$')
    {
        end -= 1;
    }
    let filter = before_cursor[end..].to_string();
    let rest = before_cursor[..end].trim_end();
    if !rest.ends_with('.') {
        if filter.is_empty() {
            return None;
        }
        return Some((String::new(), filter));
    }
    let without_dot = &rest[..rest.len() - 1];
    let path = take_trailing_receiver_expr(without_dot).unwrap_or_default();
    if path.is_empty() && filter.is_empty() {
        return None;
    }
    Some((path, filter))
}

/// Walk backward from `s` to capture a JS receiver: `ident`, `a.b`, `foo(…)`, `a.b(…).c(…)`.
fn take_trailing_receiver_expr(s: &str) -> Option<String> {
    let s = s.trim_end();
    if s.is_empty() {
        return None;
    }
    let chars: Vec<(usize, char)> = s.char_indices().collect();
    let mut i = chars.len(); // exclusive end index into `chars`
    let mut depth = 0i32;
    let mut in_str: Option<char> = None;

    while i > 0 {
        i -= 1;
        let (_, c) = chars[i];

        if let Some(q) = in_str {
            if c == q {
                // count backslashes
                let mut bs = 0;
                let mut j = i;
                while j > 0 && chars[j - 1].1 == '\\' {
                    bs += 1;
                    j -= 1;
                }
                if bs % 2 == 0 {
                    in_str = None;
                }
            }
            continue;
        }

        match c {
            '\'' | '"' | '`' => in_str = Some(c),
            ')' | ']' => depth += 1,
            '(' | '[' => {
                depth -= 1;
                if depth < 0 {
                    // Unmatched open — receiver starts after it
                    i += 1;
                    break;
                }
            }
            // Stoppers at top level (not inside call args)
            c if depth == 0
                && (c == ';'
                    || c == ','
                    || c == '='
                    || c == '{'
                    || c == '}'
                    || c == ':'
                    || c == '?'
                    || c == '!'
                    || c == '&'
                    || c == '|'
                    || c == '+'
                    || c == '-'
                    || c == '*'
                    || c == '/'
                    || c == '%'
                    || c == '<'
                    || c == '>'
                    || c.is_whitespace()) =>
            {
                i += 1; // exclude stopper
                break;
            }
            _ => {}
        }
    }

    if i >= chars.len() {
        return None;
    }
    let start_byte = chars[i].0;
    let expr = s[start_byte..].trim();
    if expr.is_empty() {
        None
    } else {
        Some(expr.to_string())
    }
}

#[cfg(test)]
mod dotted_prefix_tests {
    use super::dotted_prefix;

    #[test]
    fn bare_and_simple_member() {
        assert_eq!(
            dotted_prefix("  sign"),
            Some(("".into(), "sign".into()))
        );
        assert_eq!(
            dotted_prefix("signed."),
            Some(("signed".into(), "".into()))
        );
        assert_eq!(
            dotted_prefix("signed.au"),
            Some(("signed".into(), "au".into()))
        );
    }

    #[test]
    fn call_result_member_chain() {
        assert_eq!(
            dotted_prefix("signRequest(request, 'secret')."),
            Some(("signRequest(request, 'secret')".into(), "".into()))
        );
        assert_eq!(
            dotted_prefix("signRequest(request, 'secret').auth"),
            Some(("signRequest(request, 'secret')".into(), "auth".into()))
        );
        assert_eq!(
            dotted_prefix("crypto.createHmac('sha256', k)."),
            Some(("crypto.createHmac('sha256', k)".into(), "".into()))
        );
    }
}

#[cfg(test)]
mod official_globals_tests {
    use super::{HTTPYAC_OFFICIAL_GLOBALS, SCRIPT_ROOTS};

    #[test]
    fn official_httpyac_global_names_match_script_roots() {
        let official: Vec<_> = HTTPYAC_OFFICIAL_GLOBALS
            .iter()
            .map(|(name, _, _)| *name)
            .collect();
        let roots: Vec<_> = SCRIPT_ROOTS.iter().map(|(name, _)| *name).collect();

        assert_eq!(official.len(), 11);
        assert_eq!(official, roots);
        assert_eq!(
            official,
            vec![
                "$global",
                "$requestClient",
                "httpFile",
                "httpRegion",
                "oauth2Session",
                "request",
                "response",
                "sleep",
                "test",
                "__dirname",
                "__filename",
            ]
        );
    }
}
