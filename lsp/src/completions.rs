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

/// Top-level objects in httpyac / response scripts.
pub const SCRIPT_ROOTS: &[(&str, &str)] = &[
    ("response", "HTTP response (status, headers, body, parsedBody)"),
    ("request", "Outgoing request (url, method, headers, body)"),
    ("client", "httpyac client API (global, test, assert, log)"),
    ("console", "console.log / console.error"),
    // Common JS globals available in httpyac script VM (not full Node IntelliSense)
    ("JSON", "JSON.parse / JSON.stringify"),
    ("Math", "Math.* helpers"),
    ("Date", "Date / Date.now()"),
    ("Object", "Object.keys / assign / …"),
    ("Array", "Array.isArray / from / …"),
    ("String", "String helpers"),
    ("Number", "Number helpers"),
    ("Boolean", "Boolean()"),
    ("Error", "throw new Error(…)"),
    ("Buffer", "Node Buffer (if runtime provides it)"),
    ("require", "require('crypto' | 'fs' | …) — runtime only"),
    ("crypto", "Often via require('crypto')"),
    ("const", "JS const binding"),
    ("let", "JS let binding"),
    ("var", "JS var binding"),
    ("if", "if (…) { … }"),
    ("for", "for / for…of"),
    ("while", "while (…)"),
    ("return", "return …"),
    ("await", "await (if async context)"),
    ("async", "async function / arrow"),
    ("true", "boolean true"),
    ("false", "boolean false"),
    ("null", "null"),
    ("undefined", "undefined"),
    ("typeof", "typeof x"),
    ("new", "new Constructor(…)"),
    ("throw", "throw error"),
    ("try", "try { … } catch"),
];

/// Completions after `response.`
pub const RESPONSE_PROPS: &[(&str, &str)] = &[
    ("statusCode", "HTTP status code (number)"),
    ("status", "Status code (alias / JetBrains-style)"),
    ("statusMessage", "Reason phrase"),
    ("headers", "Response headers object"),
    ("body", "Raw body string / parsed shape (httpyac)"),
    ("parsedBody", "Parsed JSON/XML body when available"),
    ("contentType", "Content-Type header value"),
    ("rawBody", "Raw body buffer / bytes"),
    ("responseTime", "Timing (ms), if available"),
];

/// Completions after `response.headers.`
pub const RESPONSE_HEADERS_PROPS: &[(&str, &str)] = &[
    ("valueOf", "headers.valueOf(\"Name\") — JetBrains-style"),
    ("get", "headers.get(\"Name\") / bracket access"),
];

/// Completions after `JSON.`
pub const JSON_PROPS: &[(&str, &str)] = &[
    ("parse", "JSON.parse(text)"),
    ("stringify", "JSON.stringify(value, replacer?, space?)"),
];

/// Completions after `Math.`
pub const MATH_PROPS: &[(&str, &str)] = &[
    ("floor", "Math.floor(n)"),
    ("ceil", "Math.ceil(n)"),
    ("round", "Math.round(n)"),
    ("random", "Math.random()"),
    ("abs", "Math.abs(n)"),
    ("max", "Math.max(…)"),
    ("min", "Math.min(…)"),
];

/// Completions after `Object.`
pub const OBJECT_PROPS: &[(&str, &str)] = &[
    ("keys", "Object.keys(obj)"),
    ("values", "Object.values(obj)"),
    ("entries", "Object.entries(obj)"),
    ("assign", "Object.assign(target, …sources)"),
];

/// Completions after `Array.`
pub const ARRAY_PROPS: &[(&str, &str)] = &[
    ("isArray", "Array.isArray(x)"),
    ("from", "Array.from(iterable)"),
    ("of", "Array.of(…)"),
];

/// Completions after `Date.`
pub const DATE_PROPS: &[(&str, &str)] = &[
    ("now", "Date.now()"),
    ("parse", "Date.parse(dateString)"),
];

/// Common `require('…')` module names in httpyac scripts.
pub const REQUIRE_MODULES: &[&str] = &[
    "crypto",
    "fs",
    "path",
    "url",
    "querystring",
    "buffer",
    "util",
    "assert",
];

/// Completions after `request.`
pub const REQUEST_PROPS: &[(&str, &str)] = &[
    ("url", "Request URL"),
    ("method", "HTTP method"),
    ("headers", "Request headers object (mutable in pre-request)"),
    ("body", "Request body"),
];

/// Completions after `client.`
pub const CLIENT_PROPS: &[(&str, &str)] = &[
    ("global", "Persistent globals: set / get / clear"),
    ("test", "client.test(name, () => { … })"),
    ("assert", "client.assert(condition, message?)"),
    ("log", "client.log(…)"),
];

/// Completions after `client.global.`
pub const CLIENT_GLOBAL_PROPS: &[(&str, &str)] = &[
    ("set", "client.global.set(key, value)"),
    ("get", "client.global.get(key)"),
    ("clear", "client.global.clear(key?)"),
    ("clearAll", "client.global.clearAll()"),
];

/// Snippets for response / script blocks.
pub const SCRIPT_SNIPPETS: &[(&str, &str, &str)] = &[
    (
        "script-response-global",
        "> {% client.global.set(…); %}",
        "> {%\n  client.global.set(\"${1:name}\", response.parsedBody${2:});\n%}",
    ),
    (
        "script-block",
        "{{ client… }} response script",
        "{{\n  client.test(\"${1:status 200}\", () => {\n    client.assert(response.statusCode === 200);\n  });\n  client.global.set(\"${2:var}\", response.parsedBody${3:});\n}}",
    ),
    (
        "script-test",
        "client.test(…)",
        "client.test(\"${1:name}\", () => {\n  client.assert(${2:response.statusCode === 200});\n});",
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

/// True if cursor is inside a response/script region for this request block.
/// Detects: `> {% … %}`, `> {…}`, multi-line `{{ … }}` after the request, or lines under an open script.
pub fn in_script_context(lines: &[&str], line_idx: usize, before_cursor: &str) -> bool {
    let line = lines.get(line_idx).copied().unwrap_or("").trim_start();

    // Current line is a handler / script line
    if line.starts_with('>') {
        return true;
    }
    if before_cursor.contains("response.")
        || before_cursor.contains("request.")
        || before_cursor.contains("client.")
        || before_cursor.contains("console.")
    {
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

/// Dot-path prefix before cursor for property completion (e.g. "response.st" → ("response", "st")).
pub fn dotted_prefix(before_cursor: &str) -> Option<(String, String)> {
    // Take trailing identifier path: foo.bar.baz or foo.bar.
    let bytes = before_cursor.as_bytes();
    let mut end = bytes.len();
    // skip trailing incomplete ident
    while end > 0 && (bytes[end - 1].is_ascii_alphanumeric() || bytes[end - 1] == b'_') {
        end -= 1;
    }
    let filter = before_cursor[end..].to_string();
    let rest = before_cursor[..end].trim_end();
    if !rest.ends_with('.') {
        // bare word at start of script expression
        if filter.is_empty() {
            return None;
        }
        // could be typing "resp" for response
        return Some((String::new(), filter));
    }
    let without_dot = &rest[..rest.len() - 1];
    let start = without_dot
        .rfind(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '.'))
        .map(|i| i + 1)
        .unwrap_or(0);
    let path = without_dot[start..].to_string();
    if path.is_empty() && filter.is_empty() {
        return None;
    }
    Some((path, filter))
}
