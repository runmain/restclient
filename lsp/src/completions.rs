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
    // httpyac primary (see https://httpyac.github.io/guide/scripting.html)
    ("response", "HTTP response (post-request / @forceRef)"),
    ("request", "Next HTTP request (pre-request; mutable)"),
    ("client", "IntelliJ-style client (global, test, assert, log)"),
    ("console", "console.log / info / warn / error → httpyac output"),
    ("exports", "Pre-request exports.NAME → {{NAME}}"),
    ("$global", "Global variable store across requests"),
    ("test", "test(name, fn) — built-in test helper"),
    ("sleep", "await sleep(ms)"),
    ("httpFile", "Current http file"),
    ("httpRegion", "Current request region"),
    ("$requestClient", "Streaming request client"),
    ("oauth2Session", "OAuth2 session when used"),
    ("__dirname", "Directory of current module/file"),
    ("__filename", "Current file path"),
    // Common JS / Node in httpyac VM
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
    ("require", "require('crypto' | 'fs' | 'assert' | …)"),
    ("crypto", "Often via require('crypto') — also bare after require"),
    ("assert", "Often via require('assert')"),
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
    ("debugger", "debugger; for CLI debug"),
];

/// Completions after `response.` (httpyac HttpResponse + common aliases)
pub const RESPONSE_PROPS: &[(&str, &str)] = &[
    ("statusCode", "HTTP status code (number)"),
    ("status", "Status code (alias / JetBrains-style)"),
    ("statusMessage", "Reason phrase"),
    ("headers", "Response headers object"),
    ("body", "Body (string / object depending on content)"),
    ("parsedBody", "Parsed JSON/XML body when available"),
    ("prettyPrintBody", "Pretty-printed body string"),
    ("contentType", "Parsed content-type"),
    ("rawBody", "Raw body Buffer"),
    ("rawHeaders", "Raw header lines"),
    ("httpVersion", "HTTP version string"),
    ("protocol", "Protocol (HTTP, etc.)"),
    ("name", "Response / request name if set"),
    ("request", "The request object that produced this response"),
    ("timings", "Timing phases (HttpTimings)"),
    ("meta", "Extra metadata map"),
    ("tags", "Tags array"),
    ("responseTime", "Timing (ms), if available (alias)"),
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

/// `crypto.` — Node crypto (httpyac script VM). Not full tsserver; curated for common APIs.
pub const CRYPTO_PROPS: &[(&str, &str)] = &[
    ("createHmac", "crypto.createHmac(algorithm, key) → Hmac"),
    ("createHash", "crypto.createHash(algorithm) → Hash"),
    ("createSign", "crypto.createSign(algorithm) → Sign"),
    ("createVerify", "crypto.createVerify(algorithm) → Verify"),
    ("randomBytes", "crypto.randomBytes(size) → Buffer"),
    ("randomUUID", "crypto.randomUUID() → string"),
    ("pbkdf2", "crypto.pbkdf2(password, salt, iterations, keylen, digest, cb)"),
    ("pbkdf2Sync", "crypto.pbkdf2Sync(…) → Buffer"),
    ("scrypt", "crypto.scrypt(…)"),
    ("scryptSync", "crypto.scryptSync(…) → Buffer"),
    ("createCipheriv", "crypto.createCipheriv(algorithm, key, iv)"),
    ("createDecipheriv", "crypto.createDecipheriv(algorithm, key, iv)"),
    ("publicEncrypt", "crypto.publicEncrypt(key, buffer)"),
    ("privateDecrypt", "crypto.privateDecrypt(key, buffer)"),
    ("timingSafeEqual", "crypto.timingSafeEqual(a, b)"),
    ("getHashes", "crypto.getHashes() → string[]"),
    ("getCiphers", "crypto.getCiphers() → string[]"),
    ("constants", "crypto.constants"),
];

/// After `createHmac(…).` / `createHash(…).` / `.update(…).` (fluent API through digest)
pub const CRYPTO_HASH_CHAIN_PROPS: &[(&str, &str)] = &[
    (
        "update",
        "hmac/hash.update(data, inputEncoding?) → this (chain; call again or .digest)",
    ),
    (
        "digest",
        "hmac/hash.digest('hex'|'base64'|…) → string | Buffer (end of chain)",
    ),
    ("copy", "hash.copy() → Hash (Hash only; copies state)"),
];

/// `digest('…` encoding arguments (Node crypto)
pub const CRYPTO_DIGEST_ENCODINGS: &[&str] =
    &["hex", "base64", "base64url", "latin1", "binary", "utf8"];

/// Full one-shot snippets when picking createHmac / createHash from `crypto.`
pub const CRYPTO_CREATE_HMAC_SNIPPET: &str =
    "createHmac('${1:sha256}', ${2:'secret'})\n  .update(${3:data})\n  .digest('${4:base64}')";
pub const CRYPTO_CREATE_HASH_SNIPPET: &str =
    "createHash('${1:sha256}')\n  .update(${2:data})\n  .digest('${3:hex}')";
/// After `).` prefer finishing with update→digest in one step
pub const CRYPTO_UPDATE_THEN_DIGEST_SNIPPET: &str =
    "update(${1:data}).digest('${2:base64}')";

/// `fs.` common sync APIs used in scripts
pub const FS_PROPS: &[(&str, &str)] = &[
    ("readFileSync", "fs.readFileSync(path, encoding?)"),
    ("writeFileSync", "fs.writeFileSync(path, data)"),
    ("existsSync", "fs.existsSync(path)"),
    ("readdirSync", "fs.readdirSync(path)"),
    ("statSync", "fs.statSync(path)"),
    ("mkdirSync", "fs.mkdirSync(path, opts?)"),
    ("readFile", "fs.readFile(path, cb)"),
    ("writeFile", "fs.writeFile(path, data, cb)"),
];

/// `path.`
pub const PATH_PROPS: &[(&str, &str)] = &[
    ("join", "path.join(…)"),
    ("resolve", "path.resolve(…)"),
    ("basename", "path.basename(p)"),
    ("dirname", "path.dirname(p)"),
    ("extname", "path.extname(p)"),
    ("parse", "path.parse(p)"),
    ("sep", "path.sep"),
];

/// `Buffer.` static
pub const BUFFER_STATIC_PROPS: &[(&str, &str)] = &[
    ("from", "Buffer.from(data, encoding?)"),
    ("alloc", "Buffer.alloc(size)"),
    ("allocUnsafe", "Buffer.allocUnsafe(size)"),
    ("concat", "Buffer.concat(list)"),
    ("isBuffer", "Buffer.isBuffer(obj)"),
    ("byteLength", "Buffer.byteLength(string, encoding?)"),
];

/// `exports.` — httpyac pre-request script side-channel (then {{authDate}} etc.)
pub const EXPORTS_NOTE: &str =
    "httpyac pre-request: assign exports.NAME then use {{NAME}} in the request";

/// Completions after `request.` (httpyac HttpRequest / Request)
pub const REQUEST_PROPS: &[(&str, &str)] = &[
    ("url", "Request URL (mutable in pre-request)"),
    ("method", "HTTP method"),
    ("headers", "Request headers object (mutable in pre-request)"),
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

/// Completions after `client.` (IntelliJ-style + httpyac)
pub const CLIENT_PROPS: &[(&str, &str)] = &[
    ("global", "Persistent globals: set / get / clear / clearAll"),
    ("test", "client.test(name, () => { … })"),
    ("assert", "client.assert(condition, message?)"),
    ("log", "client.log(…)"),
    ("exit", "client.exit() — stop further requests (IntelliJ-style)"),
    ("isInitial", "Whether this is the first request in a run"),
];

/// Completions after `client.global.`
pub const CLIENT_GLOBAL_PROPS: &[(&str, &str)] = &[
    ("set", "client.global.set(key, value)"),
    ("get", "client.global.get(key)"),
    ("isEmpty", "client.global.isEmpty()"),
    ("clear", "client.global.clear(key?)"),
    ("clearAll", "client.global.clearAll()"),
];

/// Top-level script globals from httpyac (besides request/response/client)
pub const HTTPYAC_SCRIPT_GLOBALS: &[(&str, &str)] = &[
    ("exports", "Pre-request: exports.NAME → {{NAME}} in request"),
    ("$global", "Cross-request global store ($global.foo = …)"),
    ("test", "test(name, () => { … }) — assert/chai helpers"),
    ("sleep", "await sleep(ms)"),
    ("httpFile", "Current http file model"),
    ("httpRegion", "Current http region / request block"),
    ("$requestClient", "Stream extra body on streaming requests"),
    ("oauth2Session", "OAuth2 session when OpenID is used"),
    ("__dirname", "Directory of current file"),
    ("__filename", "Path of current file"),
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

/// True if nearby script text looks like an open crypto Hash/Hmac/Sign fluent chain.
fn crypto_fluent_window(lines: &[&str], line_idx: usize, before_cursor: &str) -> bool {
    let start = line_idx.saturating_sub(8);
    let mut window = String::new();
    for i in start..line_idx {
        window.push_str(lines.get(i).copied().unwrap_or(""));
        window.push('\n');
    }
    window.push_str(before_cursor);
    let w = window.as_str();
    let starters = [
        "createHmac",
        "createHash",
        "createSign",
        "createVerify",
        "createHmac(",
        "createHash(",
    ];
    if !starters.iter().any(|s| w.contains(s)) {
        return false;
    }
    // Still in chain if last digest(…) is not the final completed call at cursor,
    // or user is typing another .member after ).
    true
}

/// If cursor is on a crypto fluent chain (possibly multi-line), return the partial
/// member name after the last `.` (may be empty right after `.`).
///
/// Handles:
/// - `crypto.createHmac('sha256', key).`
/// - `crypto.createHmac(...).update(data).`
/// - newline + `.update` / `.digest`
pub fn crypto_hash_chain_filter(
    lines: &[&str],
    line_idx: usize,
    before_cursor: &str,
) -> Option<String> {
    if !crypto_fluent_window(lines, line_idx, before_cursor) {
        return None;
    }

    // `...).partial` on same line
    if let Some(idx) = before_cursor.rfind(").") {
        let after = &before_cursor[idx + 2..];
        if after.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            return Some(after.to_string());
        }
    }

    // Multi-line: `  .partial` or `  .`
    let trimmed = before_cursor.trim_start();
    if let Some(rest) = trimmed.strip_prefix('.') {
        if rest.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            return Some(rest.to_string());
        }
    }

    // `foo.update(` still inside call — not member complete
    None
}

/// Typing `digest('…` or `.digest("` — suggest encodings.
pub fn crypto_digest_encoding_partial(before_cursor: &str) -> Option<String> {
    // digest('  or digest("  or .digest('
    let lower = before_cursor.to_ascii_lowercase();
    let markers = [".digest(", "digest("];
    let mut pos = None;
    for m in markers {
        if let Some(i) = lower.rfind(m) {
            pos = Some(i + m.len());
            break;
        }
    }
    let pos = pos?;
    let after = before_cursor[pos..].trim_start();
    let (quote, rest) = if let Some(r) = after.strip_prefix('\'') {
        ('\'', r)
    } else if let Some(r) = after.strip_prefix('"') {
        ('"', r)
    } else {
        return None;
    };
    if rest.contains(quote) {
        return None;
    }
    Some(
        rest.chars()
            .take_while(|c| c.is_ascii_alphanumeric())
            .collect(),
    )
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
