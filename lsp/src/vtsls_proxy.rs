//! Child **vtsls** process for script islands inside `.http` buffers.
//!
//! This is **not** Zed-managed vtsls (that only attaches to real JS/TS buffers).
//! httpyac-lsp can spawn `@vtsls/language-server` (`vtsls --stdio`) **or** use the
//! built-in script catalog — **mutually exclusive** in script regions (see
//! [`ScriptCompletionSource`]).
//!
//! Configure with (priority high → low):
//! 1. `lsp.httpyac-lsp.settings.vtslsCommand`
//! 2. env `HTTPYAC_VTSLS_COMMAND`
//! 3. `PATH` → `vtsls`
//!
//! Script engine (pick **one**):
//! - `useBuiltinScriptCompletions: true` (default) → catalog only
//! - `useBuiltinScriptCompletions: false` / `scriptCompletionSource: "vtsls"` → child vtsls only

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, Command};
use tokio::sync::{oneshot, Mutex};
use tower_lsp::lsp_types::*;
use tower_lsp::Client;



/// Which engine answers **script-island** completions (Host / `{{var}}` always catalog).
/// **Mutually exclusive** — never merge both in one popup.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScriptCompletionSource {
    /// Built-in httpyac catalog (`.script` / `@returns` / require shapes).
    Builtin,
    /// Child `@vtsls/language-server` process only.
    Vtsls,
}

impl Default for ScriptCompletionSource {
    fn default() -> Self {
        // Stable default: curated tips without requiring vtsls.
        Self::Builtin
    }
}

impl ScriptCompletionSource {
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "builtin" | "catalog" | "httpyac" | "built-in" | "internal" => Some(Self::Builtin),
            "vtsls" | "ts" | "typescript" | "external" => Some(Self::Vtsls),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Builtin => "builtin",
            Self::Vtsls => "vtsls",
        }
    }
}

/// User / init settings for script completion + child vtsls.
#[derive(Debug, Clone)]
pub struct VtslsSettings {
    /// When `source == Vtsls`, allow spawning the child (false → force builtin).
    pub enabled: bool,
    /// Script-region engine: **builtin XOR vtsls** (not both).
    pub source: ScriptCompletionSource,
    /// Absolute or PATH command, e.g. `/…/bin/vtsls`.
    pub command: Option<String>,
    pub args: Vec<String>,
    /// True if `source` was set explicitly in the last `from_json` (for merge).
    source_explicit: bool,
    enabled_explicit: bool,
}

impl Default for VtslsSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            source: ScriptCompletionSource::Builtin,
            command: None,
            args: vec!["--stdio".into()],
            source_explicit: false,
            enabled_explicit: false,
        }
    }
}

impl VtslsSettings {
    /// Effective engine for script islands (applies `vtslsEnabled: false` → builtin).
    pub fn script_source(&self) -> ScriptCompletionSource {
        if !self.enabled {
            return ScriptCompletionSource::Builtin;
        }
        self.source
    }

    /// Merge JSON from `initializationOptions` or `lsp.httpyac-lsp.settings`.
    pub fn from_json(v: &Value) -> Self {
        let mut s = Self::default();

        if let Some(b) = v
            .get("vtslsEnabled")
            .or_else(|| v.get("vtsls_enabled"))
            .and_then(|x| x.as_bool())
        {
            s.enabled = b;
            s.enabled_explicit = true;
        }

        // Explicit source string (preferred)
        for key in [
            "scriptCompletionSource",
            "script_completion_source",
            "scriptEngine",
            "script_engine",
        ] {
            if let Some(raw) = v.get(key).and_then(|x| x.as_str()) {
                if let Some(src) = ScriptCompletionSource::parse(raw) {
                    s.source = src;
                    s.source_explicit = true;
                    break;
                }
            }
        }

        // Boolean: whether to use **built-in** catalog (user-facing name)
        for key in [
            "useBuiltinScriptCompletions",
            "use_builtin_script_completions",
            "useBuiltinScript",
            "use_builtin_script",
        ] {
            if let Some(b) = v.get(key).and_then(|x| x.as_bool()) {
                s.source = if b {
                    ScriptCompletionSource::Builtin
                } else {
                    ScriptCompletionSource::Vtsls
                };
                s.source_explicit = true;
                break;
            }
        }

        // camelCase + snake_case command
        for key in ["vtslsCommand", "vtsls_command"] {
            if let Some(c) = v.get(key).and_then(|x| x.as_str()) {
                let c = c.trim();
                if !c.is_empty() {
                    s.command = Some(c.to_string());
                }
            }
        }
        if let Some(arr) = v.get("vtslsArgs").and_then(|x| x.as_array()) {
            let args: Vec<String> = arr
                .iter()
                .filter_map(|x| x.as_str().map(|s| s.to_string()))
                .collect();
            if !args.is_empty() {
                s.args = args;
            }
        }
        s
    }

    pub fn merge_from(&mut self, other: &VtslsSettings) {
        if other.command.is_some() {
            self.command = other.command.clone();
        }
        if other.enabled_explicit {
            self.enabled = other.enabled;
            self.enabled_explicit = true;
        }
        if other.source_explicit {
            self.source = other.source;
            self.source_explicit = true;
        }
        if !other.args.is_empty()
            && (other.args != vec!["--stdio".to_string()] || self.args.is_empty())
        {
            self.args = other.args.clone();
        }
    }
}

/// Resolve binary path: settings → env → PATH → common npm global locations.
pub fn resolve_vtsls_command(settings_cmd: Option<&str>) -> Option<String> {
    let candidates: Vec<String> = {
        let mut v = Vec::new();
        if let Some(c) = settings_cmd.map(str::trim).filter(|s| !s.is_empty()) {
            v.push(c.to_string());
        }
        if let Ok(e) = std::env::var("HTTPYAC_VTSLS_COMMAND") {
            let e = e.trim();
            if !e.is_empty() {
                v.push(e.to_string());
            }
        }
        v.push("vtsls".into());
        // nvm / common globals (best-effort)
        if let Ok(home) = std::env::var("HOME") {
            v.push(format!("{home}/.nvm/versions/node/v24.11.1/bin/vtsls"));
            // scan a few nvm versions
            let nvm = PathBuf::from(&home).join(".nvm/versions/node");
            if let Ok(rd) = std::fs::read_dir(&nvm) {
                for ent in rd.flatten() {
                    let p = ent.path().join("bin/vtsls");
                    if p.is_file() {
                        v.push(p.to_string_lossy().to_string());
                    }
                }
            }
            v.push(format!("{home}/.local/bin/vtsls"));
        }
        v.push("/usr/local/bin/vtsls".into());
        v.push("/opt/homebrew/bin/vtsls".into());
        v
    };

    for c in candidates {
        if c == "vtsls" {
            if which_ok("vtsls") {
                return Some("vtsls".into());
            }
            continue;
        }
        let p = Path::new(&c);
        if p.is_file() {
            return Some(c);
        }
    }
    None
}

fn which_ok(name: &str) -> bool {
    let which = if cfg!(windows) { "where" } else { "which" };
    std::process::Command::new(which)
        .arg(name)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Line-aligned **plain JavaScript** (one output line per HTTP line).
///
/// No TypeScript `declare` preamble — that broke parity with real `.js` buffers
/// (`auth-sign.js`). httpyac globals live in `httpyac-vtsls-globals.d.ts`; Node
/// APIs come from `@types/node` via jsconfig (same as opening a real JS file).
///
/// Returns `(js_text, preamble_lines)` where `preamble_lines` is **0** so LSP
/// positions map 1:1 with the `.http` buffer.
///
/// Only script islands are emitted:
/// - multi-line `{{ … }}` pre/post request blocks
/// - `> {% … %}` / handler percent blocks
/// - true inline JS lines (const/require/…) — never HTTP comments like `## …`
pub fn build_virtual_typescript(http_src: &str) -> (String, u32) {
    let mut out = String::new();
    let mut in_mustache = false; // multi-line {{ … }}
    let mut in_percent = false; // {% … %}

    for line in http_src.lines() {
        let trim = line.trim();
        let mut emit = String::new();

        // ── percent handlers: > {% … %} ──
        if trim.contains("{%") {
            in_percent = true;
        }
        if in_percent {
            if trim.contains("%}") {
                in_percent = false;
                out.push('\n');
                continue;
            }
            // Skip the opening `{%` line itself; emit body only
            if !trim.contains("{%") {
                let body = line.strip_prefix('>').unwrap_or(line);
                let body = body.strip_prefix('<').unwrap_or(body);
                emit = body.to_string();
            }
            out.push_str(&emit);
            out.push('\n');
            continue;
        }

        // ── multi-line {{ script }} (httpyac pre/post request) ──
        // Single-line {{ var }} is a mustache *variable*, not a script — leave blank.
        if trim == "{{" {
            in_mustache = true;
            out.push('\n');
            continue;
        }
        if in_mustache {
            if trim == "}}" || trim.starts_with("}}") {
                in_mustache = false;
                out.push('\n');
                continue;
            }
            let body = line.strip_prefix('>').unwrap_or(line);
            let body = body.strip_prefix('<').unwrap_or(body);
            // Keep indentation so column maps stay 1:1 with .http
            emit = body.to_string();
            out.push_str(&emit);
            out.push('\n');
            continue;
        }

        // ── single-line {{ expr }} that is pure JS (rare) — not {{var}} in URL/header ──
        if trim.starts_with("{{") && trim.ends_with("}}") && trim.len() > 4 {
            let inner = trim
                .trim_start_matches("{{")
                .trim_end_matches("}}")
                .trim();
            // Only emit if it looks like real JS, not a bare variable name / path
            if looks_like_inline_script_line(inner)
                && !inner.contains('/')
                && !inner.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.')
            {
                emit = format!("  {inner}");
            } else if looks_like_js_statement(inner) {
                emit = format!("  {inner}");
            }
            out.push_str(&emit);
            out.push('\n');
            continue;
        }

        // ── bare inline JS (no braces) — strict, never HTTP comments ──
        if !trim.starts_with('#')
            && !trim.starts_with('@')
            && looks_like_inline_script_line(trim)
        {
            emit = line.to_string();
        }

        out.push_str(&emit);
        out.push('\n');
    }

    (out, 0)
}

/// True JS statement (const/let/require/exports/…) — not a bare mustache var.
fn looks_like_js_statement(s: &str) -> bool {
    let t = s.trim();
    t.starts_with("const ")
        || t.starts_with("let ")
        || t.starts_with("var ")
        || t.starts_with("function ")
        || t.starts_with("await ")
        || t.starts_with("async ")
        || t.starts_with("return ")
        || t.contains("require(")
        || t.contains("exports.")
        || t.contains("=")
}

/// Inline script heuristics. **Must not** match HTTP prose like
/// `## Uses http-client.env.json` (false positive on bare `client.`).
fn looks_like_inline_script_line(trim: &str) -> bool {
    // Never treat httpyac/markdown comments as script outside mustache blocks
    if trim.starts_with('#') {
        return false;
    }
    const SIG: &[&str] = &[
        "const ",
        "let ",
        "var ",
        "function ",
        "await ",
        "async ",
        "require(",
        "exports.",
        "response.",
        "request.",
        "console.",
        "crypto.",
        "return ",
        "Buffer.",
        "JSON.",
        "Math.",
    ];
    if SIG.iter().any(|s| trim.contains(s)) {
        return true;
    }
    // `client.` only as a free identifier (not `http-client.env`)
    if let Some(idx) = trim.find("client.") {
        let ok_before = idx == 0
            || (!trim.as_bytes()[idx - 1].is_ascii_alphanumeric()
                && trim.as_bytes()[idx - 1] != b'-'
                && trim.as_bytes()[idx - 1] != b'_');
        if ok_before {
            return true;
        }
    }
    false
}

/// Map HTTP LSP position → virtual TS position (add preamble lines).
pub fn http_pos_to_virtual(pos: Position, preamble_lines: u32) -> Position {
    Position {
        line: pos.line.saturating_add(preamble_lines),
        character: pos.character,
    }
}

pub fn virtual_pos_to_http(pos: Position, preamble_lines: u32) -> Position {
    Position {
        line: pos.line.saturating_sub(preamble_lines),
        character: pos.character,
    }
}

/// Shadow `.js` path beside the `.http` file — same folder so `require('./scripts/…')`
/// and `jsconfig.json` / `@types/node` resolve **exactly like** opening `auth-sign.js`.
/// Example: `examples/environments.http` → `examples/environments.http.__vtsls__.js`
pub fn shadow_js_path(http_uri: &Url) -> Option<PathBuf> {
    let p = http_uri.to_file_path().ok()?;
    // /tmp/httpyac-vtsls/<stable-hash>/name.http.__vtsls__.js
    // PathBuf::join ignores prefix if second is absolute — so hash the full path.
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut h = DefaultHasher::new();
    p.hash(&mut h);
    let hash = format!("{:x}", h.finish());
    let fname = p
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "doc.http".into());
    let dir = std::env::temp_dir().join("httpyac-vtsls").join(&hash);
    let _ = fs::create_dir_all(&dir);
    Some(dir.join(format!("{fname}.__vtsls__.js")))
}

fn virtual_uri_for(http_uri: &Url) -> Url {
    if let Some(path) = shadow_js_path(http_uri) {
        if let Ok(u) = Url::from_file_path(&path) {
            return u;
        }
    }
    // Fallback (non-file URIs): synthetic — weaker IntelliSense
    let s = http_uri.as_str();
    if s.ends_with(".__vtsls__.js") {
        return http_uri.clone();
    }
    Url::parse(&format!("{s}.__vtsls__.js")).unwrap_or_else(|_| http_uri.clone())
}

/// Symlink useful project entries into the tmp shadow dir so relative
/// `require('./scripts/…')` resolves like a real file next to the .http.
fn link_http_dir_into_shadow(shadow_dir: &Path, http_dir: &Path) {
    // scripts/ is the main relative-require target
    // Only link scripts/ + node_modules — NEVER project jsconfig.json
    // (linking jsconfig into tmp made Zed/json-ls open a broken path and spam errors).
    for name in ["scripts", "node_modules"] {
        let src = http_dir.join(name);
        if !src.exists() {
            continue;
        }
        let dst = shadow_dir.join(name);
        if dst.exists() || dst.symlink_metadata().is_ok() {
            continue;
        }
        #[cfg(unix)]
        {
            let _ = std::os::unix::fs::symlink(&src, &dst);
        }
        #[cfg(not(unix))]
        {
            let _ = src;
            let _ = dst;
        }
    }
}

/// Rewrite `require('./rel')` / `require("../rel")` to absolute paths based on
/// the real `.http` directory (shadow lives under $TMPDIR). Kept as fallback.
#[allow(dead_code)]
fn rewrite_relative_requires(js: &str, http_dir: &Path) -> String {
    // Minimal scanner: require('…') / require("…")
    let mut out = String::with_capacity(js.len() + 64);
    let bytes = js.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if i + 9 <= bytes.len() && &js[i..i + 8] == "require(" {
            out.push_str("require(");
            i += 8;
            // skip ws
            while i < bytes.len() && bytes[i].is_ascii_whitespace() {
                out.push(bytes[i] as char);
                i += 1;
            }
            if i < bytes.len() && (bytes[i] == b'\'' || bytes[i] == b'"') {
                let quote = bytes[i] as char;
                out.push(quote);
                i += 1;
                let start = i;
                while i < bytes.len() && bytes[i] as char != quote {
                    i += 1;
                }
                let spec = &js[start..i];
                if spec.starts_with("./") || spec.starts_with("../") {
                    let abs = http_dir.join(spec);
                    let abs = abs.canonicalize().unwrap_or(abs);
                    out.push_str(&abs.to_string_lossy());
                } else {
                    out.push_str(spec);
                }

                if i < bytes.len() {
                    out.push(quote);
                    i += 1;
                }
            }
            continue;
        }
        out.push(bytes[i] as char);
        i += 1;
    }
    out
}

/// jsconfig inside the /tmp shadow dir; baseUrl = real http folder for module resolve.
fn ensure_jsconfig_for_shadow_dir(shadow_dir: &Path, http_dir: &Path) -> Result<(), String> {
    let jsconfig = shadow_dir.join("jsconfig.json");
    let mut type_roots: Vec<String> = Vec::new();
    if http_dir.join("node_modules/@types").is_dir() {
        type_roots.push(http_dir.join("node_modules/@types").to_string_lossy().into());
    }
    if let Ok(home) = std::env::var("HOME") {
        let cache = PathBuf::from(&home).join("Library/Caches/typescript");
        if let Ok(rd) = std::fs::read_dir(&cache) {
            for ent in rd.flatten() {
                let tr = ent.path().join("node_modules/@types");
                if tr.is_dir() {
                    type_roots.push(tr.to_string_lossy().to_string());
                }
            }
        }
    }
    let mut compiler: serde_json::Map<String, Value> = serde_json::Map::new();
    compiler.insert("module".into(), json!("commonjs"));
    compiler.insert("target".into(), json!("ES2020"));
    compiler.insert("checkJs".into(), json!(true));
    compiler.insert("strict".into(), json!(false));
    compiler.insert("noEmit".into(), json!(true));
    compiler.insert("moduleResolution".into(), json!("node"));
    compiler.insert("types".into(), json!(["node"]));
    compiler.insert("baseUrl".into(), json!(http_dir.to_string_lossy()));
    if !type_roots.is_empty() {
        compiler.insert("typeRoots".into(), json!(type_roots));
    }
    let doc = json!({
        "compilerOptions": compiler,
        "include": ["./**/*.js", "./**/*.__vtsls__.js"]
    });
    // httpyac globals in the tmp dir
    let globals = shadow_dir.join("httpyac-vtsls-globals.d.ts");
    if !globals.is_file() {
        let _ = fs::write(
            &globals,
            r#"/** Auto-generated for httpyac-lsp child vtsls — httpyac script globals */
declare const request: {
  method: string;
  url: string;
  headers: Record<string, string> & { set?(k: string, v: string): void; get?(k: string): string };
  body?: unknown;
  [key: string]: unknown;
};
declare const response: {
  statusCode: number;
  status: number;
  headers: Record<string, string>;
  body: string | unknown;
  parsedBody?: unknown;
  [key: string]: unknown;
};
declare const client: {
  test(name: string, fn: () => void): void;
  assert(cond: unknown, message?: string): void;
  log(...args: unknown[]): void;
  global: { get(k: string): unknown; set(k: string, v: unknown): void; clear(k?: string): void };
  [key: string]: unknown;
};
declare const exports: Record<string, unknown>;
declare function test(name: string, fn: () => void): void;
"#,
        );
    }
    fs::write(
        &jsconfig,
        serde_json::to_string_pretty(&doc).map_err(|e| e.to_string())? + "\n",
    )
    .map_err(|e| format!("write jsconfig.json: {e}"))?;
    Ok(())
}

/// Test-only: write jsconfig into an arbitrary dir (never used for project trees in prod).
#[cfg(test)]
fn ensure_jsconfig_for_http_dir(http_dir: &Path) -> Result<(), String> {
    let jsconfig = http_dir.join("jsconfig.json");
    if jsconfig.is_file() {
        return Ok(());
    }
    // Prefer workspace / ATA @types/node so require('crypto') == real Node API
    let mut type_roots: Vec<String> = Vec::new();
    if http_dir.join("node_modules/@types").is_dir() {
        type_roots.push("./node_modules/@types".into());
    }
    if let Some(parent) = http_dir.parent() {
        if parent.join("node_modules/@types").is_dir() {
            type_roots.push("../node_modules/@types".into());
        }
    }
    // TypeScript automatic type acquisition cache (where Zed/tsserver already put @types/node)
    if let Ok(home) = std::env::var("HOME") {
        let cache = PathBuf::from(&home).join("Library/Caches/typescript");
        if let Ok(rd) = std::fs::read_dir(&cache) {
            for ent in rd.flatten() {
                let tr = ent.path().join("node_modules/@types");
                if tr.is_dir() {
                    type_roots.push(tr.to_string_lossy().to_string());
                }
            }
        }
        let xdg = PathBuf::from(&home).join(".cache/typescript");
        if let Ok(rd) = std::fs::read_dir(&xdg) {
            for ent in rd.flatten() {
                let tr = ent.path().join("node_modules/@types");
                if tr.is_dir() {
                    type_roots.push(tr.to_string_lossy().to_string());
                }
            }
        }
    }

    let mut compiler: serde_json::Map<String, Value> = serde_json::Map::new();
    compiler.insert("module".into(), json!("commonjs"));
    compiler.insert("target".into(), json!("ES2020"));
    compiler.insert("checkJs".into(), json!(true));
    compiler.insert("strict".into(), json!(false));
    compiler.insert("noEmit".into(), json!(true));
    compiler.insert("moduleResolution".into(), json!("node"));
    compiler.insert("types".into(), json!(["node"]));
    if !type_roots.is_empty() {
        compiler.insert("typeRoots".into(), json!(type_roots));
    }

    let doc = json!({
        "compilerOptions": compiler,
        "include": [
            "./**/*.js",
            "./**/*.cjs",
            "./**/*.mjs",
            "./**/*.__vtsls__.js",
            "./scripts/**/*.js"
        ]
    });
    // httpyac globals for script islands (request/response/…)
    let globals = http_dir.join("httpyac-vtsls-globals.d.ts");
    if !globals.is_file() {
        let _ = fs::write(
            &globals,
            r#"/** Auto-generated for httpyac-lsp child vtsls — httpyac script globals */
declare const request: {
  method: string;
  url: string;
  headers: Record<string, string> & { set?(k: string, v: string): void; get?(k: string): string };
  body?: unknown;
  [key: string]: unknown;
};
declare const response: {
  statusCode: number;
  status: number;
  headers: Record<string, string>;
  body: string | unknown;
  parsedBody?: unknown;
  [key: string]: unknown;
};
declare const client: {
  test(name: string, fn: () => void): void;
  assert(cond: unknown, message?: string): void;
  log(...args: unknown[]): void;
  global: { get(k: string): unknown; set(k: string, v: unknown): void; clear(k?: string): void };
  [key: string]: unknown;
};
declare const exports: Record<string, unknown>;
declare function test(name: string, fn: () => void): void;
"#,
        );
    }

    fs::write(
        &jsconfig,
        serde_json::to_string_pretty(&doc).map_err(|e| e.to_string())? + "\n",
    )
    .map_err(|e| format!("write jsconfig.json: {e}"))?;
    Ok(())
}

struct Pending {
    map: HashMap<u64, oneshot::Sender<Result<Value, String>>>,
}

/// Running child vtsls + open virtual docs.
pub struct VtslsProxy {
    #[allow(dead_code)]
    child: Child,
    stdin: Arc<Mutex<ChildStdin>>,
    pending: Arc<Mutex<Pending>>,
    next_id: AtomicU64,
    /// virtual uri string → version
    versions: Mutex<HashMap<String, i32>>,
    preamble_lines: u32,
    command_path: String,
}

impl VtslsProxy {
    pub async fn spawn(command: &str, args: &[String], workspace_root: Option<PathBuf>) -> Result<Self, String> {
        let mut cmd = Command::new(command);
        cmd.args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        if let Some(ref root) = workspace_root {
            cmd.current_dir(root);
        }

        let mut child = cmd.spawn().map_err(|e| {
            format!(
                "failed to start vtsls ({command}): {e}. Install: npm install -g @vtsls/language-server"
            )
        })?;

        let stdin = child.stdin.take().ok_or("vtsls stdin missing")?;
        let stdout = child.stdout.take().ok_or("vtsls stdout missing")?;
        let stderr = child.stderr.take();

        let pending = Arc::new(Mutex::new(Pending {
            map: HashMap::new(),
        }));
        let stdin = Arc::new(Mutex::new(stdin));
        let pending_r = pending.clone();
        let stdin_r = stdin.clone();

        // Read stdout: complete our requests + **answer server→client requests**
        // (vtsls blocks on workspace/configuration if we never reply → completion timeout).
        tokio::spawn(async move {
            if let Err(e) = read_lsp_stdout(stdout, pending_r, stdin_r).await {
                eprintln!("[httpyac-lsp] vtsls reader ended: {e}");
            }
        });

        // Drain stderr (avoid fill-up)
        if let Some(stderr) = stderr {
            tokio::spawn(async move {
                let mut r = BufReader::new(stderr);
                let mut line = String::new();
                loop {
                    line.clear();
                    match r.read_line(&mut line).await {
                        Ok(0) => break,
                        Ok(_) => {
                            let t = line.trim();
                            if !t.is_empty() {
                                eprintln!("[vtsls] {t}");
                            }
                        }
                        Err(_) => break,
                    }
                }
            });
        }

        let proxy = Self {
            child,
            stdin,
            pending,
            next_id: AtomicU64::new(1),
            versions: Mutex::new(HashMap::new()),
            // Line-aligned plain JS shadow → positions match .http 1:1 (no TS preamble).
            preamble_lines: 0,
            command_path: command.to_string(),
        };

        proxy.initialize(workspace_root).await?;
        // Brief settle so tsserver project service can start
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        Ok(proxy)
    }

    async fn initialize(&self, workspace_root: Option<PathBuf>) -> Result<(), String> {
        let root = workspace_root.unwrap_or_else(|| PathBuf::from("."));
        let root_uri = Url::from_file_path(&root).unwrap_or_else(|_| Url::parse("file:///").unwrap());
        let params = json!({
            "processId": std::process::id(),
            "clientInfo": { "name": "httpyac-lsp", "version": "0.1.0" },
            "rootUri": root_uri,
            "rootPath": root.to_string_lossy(),
            "capabilities": {
                "textDocument": {
                    "completion": {
                        "completionItem": {
                            "snippetSupport": true,
                            "documentationFormat": ["markdown", "plaintext"]
                        },
                        "contextSupport": true
                    },
                    "hover": { "contentFormat": ["markdown", "plaintext"] },
                    "definition": { "linkSupport": true }
                },
                "workspace": {
                    "workspaceFolders": true,
                    "configuration": true
                }
            },
            "workspaceFolders": [{ "uri": root_uri, "name": "httpyac" }],
            "initializationOptions": {
                "typescript": {
                    "tsdk": null
                }
            }
        });
        let _ = self.request("initialize", params).await?;
        self.notify("initialized", json!({})).await?;
        Ok(())
    }

    pub fn command_path(&self) -> &str {
        &self.command_path
    }

    pub fn preamble_lines(&self) -> u32 {
        self.preamble_lines
    }

    async fn write_message(&self, body: &Value) -> Result<(), String> {
        let data = serde_json::to_vec(body).map_err(|e| e.to_string())?;
        let header = format!("Content-Length: {}\r\n\r\n", data.len());
        let mut stdin = self.stdin.lock().await;
        stdin
            .write_all(header.as_bytes())
            .await
            .map_err(|e| e.to_string())?;
        stdin.write_all(&data).await.map_err(|e| e.to_string())?;
        stdin.flush().await.map_err(|e| e.to_string())?;
        Ok(())
    }

    pub async fn notify(&self, method: &str, params: Value) -> Result<(), String> {
        self.write_message(&json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
        }))
        .await
    }

    pub async fn request(&self, method: &str, params: Value) -> Result<Value, String> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let (tx, rx) = oneshot::channel();
        {
            let mut p = self.pending.lock().await;
            p.map.insert(id, tx);
        }
        self.write_message(&json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        }))
        .await?;

        match tokio::time::timeout(std::time::Duration::from_secs(15), rx).await {
            Ok(Ok(Ok(v))) => Ok(v),
            Ok(Ok(Err(e))) => Err(e),
            Ok(Err(_)) => Err("vtsls response channel closed".into()),
            Err(_) => {
                let mut p = self.pending.lock().await;
                p.map.remove(&id);
                Err(format!("vtsls request timed out ({method})"))
            }
        }
    }

    /// Sync script islands into a **real on-disk `.js` shadow file** under
    /// `$TMPDIR/httpyac-vtsls/<hash>/` (not the project tree), then tell vtsls.
    pub async fn sync_document(&self, http_uri: &Url, http_text: &str) -> Result<(), String> {
        let (virtual_text, preamble) = build_virtual_typescript(http_text);
        debug_assert_eq!(preamble, 0, "shadow JS must stay line-aligned with .http");

        // Materialize under $TMPDIR; symlink real http dir entries so
        // require('./scripts/…') still works without rewriting paths.
        if let Some(shadow) = shadow_js_path(http_uri) {
            if let Ok(http_path) = http_uri.to_file_path() {
                if let Some(http_dir) = http_path.parent() {
                    if let Some(dir) = shadow.parent() {
                        let _ = fs::create_dir_all(dir);
                        link_http_dir_into_shadow(dir, http_dir);
                        let _ = ensure_jsconfig_for_shadow_dir(dir, http_dir);
                    }
                }
            }
            if let Some(parent) = shadow.parent() {
                let _ = fs::create_dir_all(parent);
            }
            fs::write(&shadow, &virtual_text)
                .map_err(|e| format!("write shadow {}: {e}", shadow.display()))?;
        }

        let vuri = virtual_uri_for(http_uri);
        let key = vuri.to_string();
        let mut versions = self.versions.lock().await;
        let is_new = !versions.contains_key(&key);
        let ver = versions.entry(key.clone()).or_insert(0);
        *ver += 1;
        let version = *ver;
        drop(versions);

        // javascript — same languageId Zed uses for auth-sign.js
        if is_new {
            self.notify(
                "textDocument/didOpen",
                json!({
                    "textDocument": {
                        "uri": vuri,
                        "languageId": "javascript",
                        "version": version,
                        "text": virtual_text,
                    }
                }),
            )
            .await?;
            // Pick up newly written jsconfig.json / @types/node
            let _ = self
                .request(
                    "workspace/executeCommand",
                    json!({
                        "command": "typescript.reloadProjects",
                        "arguments": []
                    }),
                )
                .await;
            let _ = self
                .request(
                    "workspace/executeCommand",
                    json!({
                        "command": "javascript.reloadProjects",
                        "arguments": []
                    }),
                )
                .await;
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        } else {
            self.notify(
                "textDocument/didChange",
                json!({
                    "textDocument": { "uri": vuri, "version": version },
                    "contentChanges": [{ "text": virtual_text }]
                }),
            )
            .await?;
        }
        Ok(())
    }

    pub async fn close_document(&self, http_uri: &Url) -> Result<(), String> {
        let vuri = virtual_uri_for(http_uri);
        let key = vuri.to_string();
        let mut versions = self.versions.lock().await;
        if versions.remove(&key).is_none() {
            return Ok(());
        }
        drop(versions);
        let _ = self
            .notify(
                "textDocument/didClose",
                json!({ "textDocument": { "uri": vuri } }),
            )
            .await;
        // Keep shadow on disk for debugging; gitignored. Uncomment to delete:
        // if let Some(p) = shadow_js_path(http_uri) { let _ = fs::remove_file(p); }
        Ok(())
    }

    pub async fn completion(
        &self,
        http_uri: &Url,
        position: Position,
        context: Option<CompletionContext>,
    ) -> Result<Option<CompletionResponse>, String> {
        let vuri = virtual_uri_for(http_uri);
        let vpos = http_pos_to_virtual(position, self.preamble_lines);
        let mut params = json!({
            "textDocument": { "uri": vuri },
            "position": { "line": vpos.line, "character": vpos.character },
        });
        if let Some(ctx) = context {
            if let Ok(v) = serde_json::to_value(ctx) {
                params["context"] = v;
            }
        }
        let raw = self.request("textDocument/completion", params).await?;
        Ok(map_completion_response(raw, self.preamble_lines))
    }

    /// Forward `completionItem/resolve` to child vtsls so detail + documentation
    /// match native JS (auth-sign.js). Initial completion often omits docs.
    ///
    /// With `completeFunctionCalls: true`, resolve fills insert_text as a snippet
    /// e.g. `.createHmac(${1:algorithm}, ${2:key}$3)$0`. Zed prefers `text_edit`
    /// over `insert_text`, so we must copy the snippet into text_edit.new_text
    /// (keeping the original range) — otherwise Enter only inserts the bare name.
    pub async fn completion_resolve(&self, item: CompletionItem) -> Result<CompletionItem, String> {
        let keep_label = item.label.clone();
        let keep_filter = item.filter_text.clone();
        let keep_sort = item.sort_text.clone();
        let keep_range = item.text_edit.as_ref().map(|e| match e {
            CompletionTextEdit::Edit(t) => t.range,
            CompletionTextEdit::InsertAndReplace(ir) => ir.replace,
        });
        let raw_item = serde_json::to_value(&item).map_err(|e| e.to_string())?;
        let raw = self.request("completionItem/resolve", raw_item).await?;
        if raw.is_null() {
            return Ok(item);
        }
        let mut resolved: CompletionItem = serde_json::from_value(raw)
            .map_err(|e| format!("completion resolve decode: {e}"))?;
        fix_completion_item_ranges(&mut resolved, self.preamble_lines);
        // Preserve fuzzy-match fields Zed already committed to
        resolved.label = keep_label;
        if keep_filter.is_some() {
            resolved.filter_text = keep_filter;
        }
        if keep_sort.is_some() {
            resolved.sort_text = keep_sort;
        }
        // Apply completeFunctionCalls snippet into text_edit (Zed uses text_edit on accept)
        apply_function_call_snippet(&mut resolved, keep_range);
        tag_vtsls_origin(&mut resolved);
        Ok(resolved)
    }

    pub async fn hover(
        &self,
        http_uri: &Url,
        position: Position,
    ) -> Result<Option<Hover>, String> {
        let vuri = virtual_uri_for(http_uri);
        let vpos = http_pos_to_virtual(position, self.preamble_lines);
        let raw = self
            .request(
                "textDocument/hover",
                json!({
                    "textDocument": { "uri": vuri },
                    "position": { "line": vpos.line, "character": vpos.character },
                }),
            )
            .await?;
        if raw.is_null() {
            return Ok(None);
        }
        let mut hover: Hover =
            serde_json::from_value(raw).map_err(|e| format!("hover decode: {e}"))?;
        if let Some(Range { start, end }) = hover.range.as_mut() {
            *start = virtual_pos_to_http(*start, self.preamble_lines);
            *end = virtual_pos_to_http(*end, self.preamble_lines);
        }
        Ok(Some(hover))
    }

    pub async fn goto_definition(
        &self,
        http_uri: &Url,
        position: Position,
    ) -> Result<Option<GotoDefinitionResponse>, String> {
        let vuri = virtual_uri_for(http_uri);
        let vpos = http_pos_to_virtual(position, self.preamble_lines);
        let raw = self
            .request(
                "textDocument/definition",
                json!({
                    "textDocument": { "uri": vuri },
                    "position": { "line": vpos.line, "character": vpos.character },
                }),
            )
            .await?;
        if raw.is_null() {
            return Ok(None);
        }
        // Remap only locations that point at our virtual uri back to http; leave real .ts/.js as-is
        remap_goto_definition(raw, http_uri, &vuri, self.preamble_lines)
    }
}

/// If resolve gave a snippet insert_text with `(…)`, put it into text_edit so
/// Enter inserts `createHmac(${1:algorithm}, ${2:key})` not bare `createHmac`.
fn apply_function_call_snippet(item: &mut CompletionItem, keep_range: Option<Range>) {
    let snippet = match item.insert_text.as_ref() {
        Some(s) if s.contains('(') => s.clone(),
        _ => return,
    };
    if item.insert_text_format.is_none()
        || item.insert_text_format == Some(InsertTextFormat::PLAIN_TEXT)
    {
        if snippet.contains("${") || snippet.contains("$0") || snippet.contains("$1") {
            item.insert_text_format = Some(InsertTextFormat::SNIPPET);
        }
    }
    match item.text_edit.as_mut() {
        Some(CompletionTextEdit::Edit(e)) => {
            if !e.new_text.contains('(') {
                e.new_text = snippet;
            }
            if let Some(r) = keep_range {
                e.range = r;
            }
        }
        Some(CompletionTextEdit::InsertAndReplace(ir)) => {
            if !ir.new_text.contains('(') {
                ir.new_text = snippet;
            }
        }
        None => {
            if let Some(r) = keep_range {
                item.text_edit = Some(CompletionTextEdit::Edit(TextEdit {
                    range: r,
                    new_text: snippet,
                }));
            }
        }
    }
}

fn map_completion_response(raw: Value, preamble: u32) -> Option<CompletionResponse> {
    if raw.is_null() {
        return None;
    }
    // array of items
    if let Some(arr) = raw.as_array() {
        let items: Vec<CompletionItem> = arr
            .iter()
            .filter_map(|v| serde_json::from_value::<CompletionItem>(v.clone()).ok())
            .map(|mut it| {
                fix_completion_item_ranges(&mut it, preamble);
                // Mark source so UI shows this came from child vtsls, not catalog.
                tag_vtsls_origin(&mut it);
                if it.sort_text.is_none() {
                    it.sort_text = Some(format!("0{}", it.label));
                }
                // The real filter_text rewrite (generic for any require()) is done in main.rs completion_inner
                // after vtsls returns items, using member_filter_label(&path, name)
                // where path comes from dotted_prefix(before_cursor)
                it
            })
            .collect();
        if items.is_empty() {
            return None;
        }
        return Some(CompletionResponse::Array(items));
    }
    // CompletionList
    if let Ok(mut list) = serde_json::from_value::<CompletionList>(raw) {
        for it in &mut list.items {
            fix_completion_item_ranges(it, preamble);
            tag_vtsls_origin(it);
            if it.sort_text.is_none() {
                it.sort_text = Some(format!("0{}", it.label));
            }
            // filter_text rewrite (generic for any require()) is done in main.rs completion_inner
        }
        if list.items.is_empty() {
            return None;
        }
        return Some(CompletionResponse::List(list));
    }
    None
}

/// Keep vtsls/tsserver completion UI **as-is** (same as opening auth-sign.js):
/// - `detail`: function signature / type (`function createHmac(...): Hmac`)
/// - `documentation`: real JSDoc / @types docs on the right pane
///
/// Do **not** overwrite with "[vtsls] via httpyac-lsp…" — that is what made the
/// popup look like garbage compared to native JS completions.
fn tag_vtsls_origin(item: &mut CompletionItem) {
    // Only strip private commands Zed cannot execute; leave label/detail/docs alone.
    if item
        .command
        .as_ref()
        .is_some_and(|c| c.command.starts_with("_vtsls."))
    {
        item.command = None;
    }
}

fn fix_completion_item_ranges(item: &mut CompletionItem, preamble: u32) {
    if let Some(CompletionTextEdit::Edit(edit)) = item.text_edit.as_mut() {
        edit.range.start = virtual_pos_to_http(edit.range.start, preamble);
        edit.range.end = virtual_pos_to_http(edit.range.end, preamble);
    }
    if let Some(CompletionTextEdit::InsertAndReplace(ir)) = item.text_edit.as_mut() {
        ir.insert.start = virtual_pos_to_http(ir.insert.start, preamble);
        ir.insert.end = virtual_pos_to_http(ir.insert.end, preamble);
        ir.replace.start = virtual_pos_to_http(ir.replace.start, preamble);
        ir.replace.end = virtual_pos_to_http(ir.replace.end, preamble);
    }
    if let Some(edits) = item.additional_text_edits.as_mut() {
        for e in edits {
            e.range.start = virtual_pos_to_http(e.range.start, preamble);
            e.range.end = virtual_pos_to_http(e.range.end, preamble);
        }
    }
}

fn remap_goto_definition(
    raw: Value,
    http_uri: &Url,
    virtual_uri: &Url,
    preamble: u32,
) -> Result<Option<GotoDefinitionResponse>, String> {
    let vuri_s = virtual_uri.as_str();
    let map_loc = |loc: &mut Location| {
        if loc.uri.as_str() == vuri_s {
            loc.uri = http_uri.clone();
            loc.range.start = virtual_pos_to_http(loc.range.start, preamble);
            loc.range.end = virtual_pos_to_http(loc.range.end, preamble);
        }
    };
    let map_link = |link: &mut LocationLink| {
        if link.target_uri.as_str() == vuri_s {
            link.target_uri = http_uri.clone();
            link.target_range.start = virtual_pos_to_http(link.target_range.start, preamble);
            link.target_range.end = virtual_pos_to_http(link.target_range.end, preamble);
            link.target_selection_range.start =
                virtual_pos_to_http(link.target_selection_range.start, preamble);
            link.target_selection_range.end =
                virtual_pos_to_http(link.target_selection_range.end, preamble);
        }
        if let Some(origin) = link.origin_selection_range.as_mut() {
            origin.start = virtual_pos_to_http(origin.start, preamble);
            origin.end = virtual_pos_to_http(origin.end, preamble);
        }
    };

    if let Ok(mut loc) = serde_json::from_value::<Location>(raw.clone()) {
        map_loc(&mut loc);
        return Ok(Some(GotoDefinitionResponse::Scalar(loc)));
    }
    if let Ok(mut locs) = serde_json::from_value::<Vec<Location>>(raw.clone()) {
        for l in &mut locs {
            map_loc(l);
        }
        return Ok(Some(GotoDefinitionResponse::Array(locs)));
    }
    if let Ok(mut links) = serde_json::from_value::<Vec<LocationLink>>(raw) {
        for l in &mut links {
            map_link(l);
        }
        return Ok(Some(GotoDefinitionResponse::Link(links)));
    }
    Ok(None)
}

async fn write_raw_message(
    stdin: &Arc<Mutex<ChildStdin>>,
    body: &Value,
) -> Result<(), String> {
    let data = serde_json::to_vec(body).map_err(|e| e.to_string())?;
    let header = format!("Content-Length: {}\r\n\r\n", data.len());
    let mut guard = stdin.lock().await;
    guard
        .write_all(header.as_bytes())
        .await
        .map_err(|e| e.to_string())?;
    guard.write_all(&data).await.map_err(|e| e.to_string())?;
    guard.flush().await.map_err(|e| e.to_string())?;
    Ok(())
}

fn json_id_as_u64(id: &Value) -> Option<u64> {
    id.as_u64()
        .or_else(|| id.as_i64().map(|i| i as u64))
        .or_else(|| id.as_str().and_then(|s| s.parse().ok()))
}

/// Default replies so vtsls does not block waiting on the client.
fn server_request_result(method: &str, params: &Value) -> Value {
    match method {
        "workspace/configuration" => {
            // Mirror settings that make real .js buffers (auth-sign.js) useful.
            let n = params
                .get("items")
                .and_then(|i| i.as_array())
                .map(|a| a.len())
                .unwrap_or(1);
            // completeFunctionCalls: true → accept insert is createHmac($1, $2) like auth-sign.js
            // (was false → only bare name createHmac)
            let cfg = json!({
                "typescript": {
                    "suggest": {
                        "enabled": true,
                        "paths": true,
                        "autoImports": true,
                        "completeFunctionCalls": true
                    },
                    "tsserver": { "useSyntaxServer": "auto" },
                    "preferences": {
                        "includePackageJsonAutoImports": "on",
                        "includeCompletionsWithSnippetText": true,
                        "includeCompletionsForModuleExports": true
                    },
                    "disableAutomaticTypeAcquisition": false
                },
                "javascript": {
                    "suggest": {
                        "enabled": true,
                        "paths": true,
                        "autoImports": true,
                        "names": true,
                        "completeFunctionCalls": true
                    },
                    "preferences": {
                        "includePackageJsonAutoImports": "on",
                        "includeCompletionsWithSnippetText": true,
                        "includeCompletionsForModuleExports": true
                    },
                    "validate": { "enable": true }
                },
                // Implicit project when no jsconfig (backup)
                "js/ts.implicitProjectConfig.checkJs": true,
                "js/ts.implicitProjectConfig.module": "CommonJS",
                "js/ts.implicitProjectConfig.target": "ES2020",
                "js/ts.implicitProjectConfig.strict": false,
                "vtsls": {
                    "experimental": {
                        "completion": {
                            "enableServerSideFuzzyMatch": true,
                            "entriesLimit": 100
                        }
                    }
                }
            });
            // Per-item: if client asks for a section, still return full map (vtsls merges).
            let items = params.get("items").and_then(|i| i.as_array());
            if let Some(arr) = items {
                let mut out = Vec::with_capacity(arr.len());
                for it in arr {
                    let section = it.get("section").and_then(|s| s.as_str()).unwrap_or("");
                    if section.is_empty() {
                        out.push(cfg.clone());
                    } else if let Some(v) = cfg.pointer(&format!("/{}", section.replace('.', "/"))) {
                        out.push(v.clone());
                    } else if section.starts_with("typescript")
                        || section.starts_with("javascript")
                        || section.starts_with("vtsls")
                        || section.starts_with("js/ts")
                    {
                        out.push(cfg.clone());
                    } else {
                        out.push(cfg.clone());
                    }
                }
                Value::Array(out)
            } else {
                Value::Array(vec![cfg; n.max(1)])
            }
        }
        "workspace/workspaceFolders" => Value::Null,
        "window/workDoneProgress/create" => Value::Null,
        "client/registerCapability" | "client/unregisterCapability" => Value::Null,
        "window/showMessageRequest" => Value::Null,
        _ => Value::Null,
    }
}

async fn read_lsp_stdout(
    stdout: tokio::process::ChildStdout,
    pending: Arc<Mutex<Pending>>,
    stdin: Arc<Mutex<ChildStdin>>,
) -> Result<(), String> {
    let mut reader = BufReader::new(stdout);
    loop {
        // headers
        let mut content_length: Option<usize> = None;
        loop {
            let mut line = String::new();
            let n = reader.read_line(&mut line).await.map_err(|e| e.to_string())?;
            if n == 0 {
                return Ok(());
            }
            let t = line.trim_end();
            if t.is_empty() {
                break;
            }
            if let Some(rest) = t
                .strip_prefix("Content-Length:")
                .or_else(|| t.strip_prefix("content-length:"))
            {
                content_length = rest.trim().parse().ok();
            }
        }
        // Missing Content-Length: skip garbage (do NOT kill the reader — that
        // used to end the task and make all later completions hang / look like crash).
        let Some(len) = content_length else {
            continue;
        };
        if len == 0 || len > 64 * 1024 * 1024 {
            continue;
        }
        let mut buf = vec![0u8; len];
        if let Err(_e) = reader.read_exact(&mut buf).await {
            // Child closed or partial — exit cleanly, don't poison parent LSP
            return Ok(());
        }
        let msg: Value = match serde_json::from_slice(&buf) {
            Ok(v) => v,
            Err(_) => continue,
        };

        let has_id = msg.get("id").is_some();
        let method = msg.get("method").and_then(|m| m.as_str());

        if has_id && method.is_some() {
            // Server → client **request** — must respond or tsserver stalls.
            let method = method.unwrap();
            let id = msg.get("id").cloned().unwrap_or(Value::Null);
            let params = msg.get("params").cloned().unwrap_or(Value::Null);
            let result = server_request_result(method, &params);
            let _ = write_raw_message(
                &stdin,
                &json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": result,
                }),
            )
            .await;
            continue;
        }

        if has_id && method.is_none() {
            // Response to **our** request
            if let Some(id_n) = msg.get("id").and_then(json_id_as_u64) {
                let mut p = pending.lock().await;
                if let Some(tx) = p.map.remove(&id_n) {
                    if let Some(err) = msg.get("error") {
                        let _ = tx.send(Err(err.to_string()));
                    } else {
                        let result = msg.get("result").cloned().unwrap_or(Value::Null);
                        let _ = tx.send(Ok(result));
                    }
                }
            }
            continue;
        }

        // Notifications (publishDiagnostics, logMessage, …) — ignore
    }
}

/// Shared handle held by HttpLsp.
pub struct VtslsBridge {
    pub settings: Mutex<VtslsSettings>,
    pub proxy: Mutex<Option<VtslsProxy>>,
    /// Once we logged "vtsls missing"
    pub warned_missing: Mutex<bool>,
}

impl VtslsBridge {
    pub fn new() -> Self {
        Self {
            settings: Mutex::new(VtslsSettings::default()),
            proxy: Mutex::new(None),
            warned_missing: Mutex::new(false),
        }
    }

    pub async fn apply_settings_json(&self, v: &Value) {
        let parsed = VtslsSettings::from_json(v);
        let mut s = self.settings.lock().await;
        s.merge_from(&parsed);
    }

    /// Current script completion engine (builtin | vtsls).
    pub async fn script_source(&self) -> ScriptCompletionSource {
        self.settings.lock().await.script_source()
    }

    pub async fn ensure_started(
        &self,
        client: &Client,
        workspace_root: Option<PathBuf>,
    ) -> Option<()> {
        {
            let s = self.settings.lock().await;
            // Only spawn when user chose vtsls engine
            if s.script_source() != ScriptCompletionSource::Vtsls {
                return None;
            }
            if !s.enabled {
                return None;
            }
        }
        {
            let p = self.proxy.lock().await;
            if p.is_some() {
                return Some(());
            }
        }

        let cmd = {
            let s = self.settings.lock().await;
            resolve_vtsls_command(s.command.as_deref())
        };
        let Some(cmd) = cmd else {
            let mut w = self.warned_missing.lock().await;
            if !*w {
                *w = true;
                client
                    .log_message(
                        MessageType::WARNING,
                        "httpyac-lsp: vtsls not found — script IntelliSense uses catalog only. \
                         Install: npm install -g @vtsls/language-server \
                         then set httpyac.vtsls_command or lsp.httpyac-lsp.settings.vtslsCommand \
                         (re-run ./install_to_zed.sh to probe/write paths).",
                    )
                    .await;
            }
            return None;
        };
        let args = {
            let s = self.settings.lock().await;
            s.args.clone()
        };

        match VtslsProxy::spawn(&cmd, &args, workspace_root).await {
            Ok(proxy) => {
                client
                    .log_message(
                        MessageType::INFO,
                        format!(
                            "httpyac-lsp: script-region vtsls ready ({})",
                            proxy.command_path()
                        ),
                    )
                    .await;
                *self.proxy.lock().await = Some(proxy);
                Some(())
            }
            Err(e) => {
                client
                    .log_message(MessageType::WARNING, format!("httpyac-lsp: {e}"))
                    .await;
                None
            }
        }
    }

    pub async fn sync(&self, uri: &Url, text: &str) {
        let p = self.proxy.lock().await;
        if let Some(proxy) = p.as_ref() {
            let _ = proxy.sync_document(uri, text).await;
        }
    }

    pub async fn close(&self, uri: &Url) {
        let p = self.proxy.lock().await;
        if let Some(proxy) = p.as_ref() {
            let _ = proxy.close_document(uri).await;
        }
    }
}

impl Default for VtslsBridge {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn virtual_doc_keeps_script_lines_aligned() {
        let src = r#"
###
GET https://example.com
{{
  const crypto = require('crypto');
  const h = crypto.createHmac('sha256', 'k');
}}
Host: x
"#;
        let (virt, preamble) = build_virtual_typescript(src);
        assert_eq!(preamble, 0, "plain JS shadow is 1:1 with .http lines");
        let http_lines = src.lines().count();
        assert_eq!(virt.lines().count(), http_lines);
        assert!(virt.contains("createHmac"));
        assert!(!virt.contains("GET https://"));
        // Must stay plain JS (no TS declare) — same as auth-sign.js
        assert!(!virt.contains("declare const"));
    }

    #[test]
    fn position_roundtrip() {
        let p = Position {
            line: 10,
            character: 4,
        };
        let v = http_pos_to_virtual(p, 20);
        assert_eq!(v.line, 30);
        assert_eq!(virtual_pos_to_http(v, 20).line, 10);
    }

    #[test]
    fn settings_parse_snake_and_camel() {
        let v = json!({"vtsls_command": "/bin/vtsls", "vtslsEnabled": true});
        let s = VtslsSettings::from_json(&v);
        assert_eq!(s.command.as_deref(), Some("/bin/vtsls"));
        assert!(s.enabled);
        // default engine remains builtin unless toggled
        assert_eq!(s.script_source(), ScriptCompletionSource::Builtin);
    }

    #[test]
    fn settings_use_builtin_flag_and_source_string() {
        let builtin = VtslsSettings::from_json(&json!({
            "useBuiltinScriptCompletions": true
        }));
        assert_eq!(builtin.script_source(), ScriptCompletionSource::Builtin);

        let vtsls = VtslsSettings::from_json(&json!({
            "useBuiltinScriptCompletions": false,
            "vtslsCommand": "/bin/vtsls"
        }));
        assert_eq!(vtsls.script_source(), ScriptCompletionSource::Vtsls);

        let by_str = VtslsSettings::from_json(&json!({
            "scriptCompletionSource": "vtsls"
        }));
        assert_eq!(by_str.script_source(), ScriptCompletionSource::Vtsls);

        let disabled = VtslsSettings::from_json(&json!({
            "scriptCompletionSource": "vtsls",
            "vtslsEnabled": false
        }));
        assert_eq!(disabled.script_source(), ScriptCompletionSource::Builtin);
    }

    #[test]
    fn shadow_path_and_jsconfig_match_real_js_project_shape() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("examples");
        let http = root.join("script-vtsls.http");
        let uri = Url::from_file_path(&http).expect("file uri");
        let shadow = shadow_js_path(&uri).expect("shadow path");
        assert!(
            shadow
                .file_name()
                .and_then(|s| s.to_str())
                .is_some_and(|n| n.ends_with(".__vtsls__.js")),
            "shadow must be *.__vtsls__.js beside .http, got {}",
            shadow.display()
        );
        // Shadow lives under $TMPDIR/httpyac-vtsls/<hash>/ (not project dir)
        let parent = shadow.parent().expect("parent");
        assert!(
            parent.to_string_lossy().contains("httpyac-vtsls"),
            "shadow must be under httpyac-vtsls tmp dir, got {}",
            parent.display()
        );

        // Ensure jsconfig materialization (idempotent if already present from prior runs)
        let dir = http.parent().unwrap();
        // Use a temp subdir so we don't clobber a real examples/jsconfig the user may want
        let tmp = std::env::temp_dir().join(format!(
            "httpyac-vtsls-jsconfig-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).unwrap();
        ensure_jsconfig_for_http_dir(&tmp).expect("jsconfig");
        let jc = fs::read_to_string(tmp.join("jsconfig.json")).expect("read jsconfig");
        assert!(
            jc.contains("\"node\"") || jc.contains("types"),
            "jsconfig must request @types/node: {jc}"
        );
        assert!(
            jc.contains("commonjs") || jc.contains("CommonJS") || jc.contains("commonjs"),
            "jsconfig module should be commonjs-like: {jc}"
        );
        assert!(
            tmp.join("httpyac-vtsls-globals.d.ts").is_file(),
            "httpyac globals d.ts must exist for request/response"
        );
        let _ = fs::remove_dir_all(&tmp);
        let _ = dir; // silence
    }

    #[test]
    fn http_cursor_maps_1to1_when_preamble_zero() {
        // Shipped proxy uses preamble_lines=0 for plain JS shadows
        let http_line = 31u32;
        let http_col = 9u32;
        let pos = Position {
            line: http_line,
            character: http_col,
        };
        let v = http_pos_to_virtual(pos, 0);
        assert_eq!(v.line, http_line);
        assert_eq!(v.character, http_col);
        let back = virtual_pos_to_http(v, 0);
        assert_eq!(back.line, http_line);
        assert_eq!(back.character, http_col);
    }

    #[test]
    fn environments_http_shadow_has_script_not_header_noise() {
        // Synthetic buffer matching user shape (crypto. after require) — not disk dirty state
        let http = "### POST\n{{\n  const crypto = require('crypto');\n  crypto.\n  const date = new Date();\n}}\n";
        let (virt, preamble) = build_virtual_typescript(http);
        assert_eq!(preamble, 0);
        assert_eq!(virt.lines().count(), http.lines().count(), "1:1 line align");
        assert!(virt.contains("require('crypto')"), "missing require: {virt}");
        assert!(
            virt.lines().any(|l| l.trim().starts_with("crypto.")),
            "missing crypto. line: {virt}"
        );
        // False positive fixed: HTTP comment with http-client.env must NOT leak
        let noisy = "## Uses http-client.env.json in this folder\n{{\n  const x = 1;\n}}\n";
        let (v2, _) = build_virtual_typescript(noisy);
        assert!(
            !v2.contains("http-client.env"),
            "header comment polluted shadow via client. false positive:\n{v2}"
        );
        assert!(!virt.contains("GET {{"), "HTTP lines must be blanked");
        assert!(!virt.contains("POST {{"), "HTTP lines must be blanked");
    }


    #[test]
    fn client_dot_false_positive_not_in_http_comment() {
        assert!(!looks_like_inline_script_line(
            "## Uses http-client.env.json in this folder (keys: dev, prod, staging)."
        ));
        assert!(looks_like_inline_script_line("client.test('x', () => {})"));
        assert!(looks_like_inline_script_line("const crypto = require('crypto');"));
        assert!(looks_like_inline_script_line("crypto."));
    }
}

#[cfg(test)]
mod live_vtsls {
    use super::*;
    use tower_lsp::lsp_types::Position;

    #[tokio::test]
    async fn live_completion_signed_partial() {
        let cmd = resolve_vtsls_command(None).expect("vtsls on PATH");
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).parent().unwrap().join("examples");
        let proxy = VtslsProxy::spawn(&cmd, &["--stdio".into()], Some(root.clone()))
            .await
            .expect("spawn");
        let http = r#"###
{{
  const signed = { authDate: 'x', authentication: 'y' };
  signe
}}
"#;
        let uri = Url::from_file_path(root.join("script-vtsls.http")).unwrap();
        proxy.sync_document(&uri, http).await.expect("sync");
        // position on "signe" — line index: ###=0, {{=1, const=2, signe=3
        // "  signe" — cursor after the word (col 7)
        let pos = Position {
            line: 3,
            character: 7,
        };
        tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
        let res = proxy
            .completion(&uri, pos, None)
            .await
            .expect("completion ok");
        eprintln!("completion res = {:?}", res);
        let labels = match res {
            Some(CompletionResponse::Array(items)) => items.into_iter().map(|i| i.label).collect::<Vec<_>>(),
            Some(CompletionResponse::List(l)) => l.items.into_iter().map(|i| i.label).collect::<Vec<_>>(),
            None => vec![],
        };
        eprintln!("labels={labels:?}");
        assert!(
            labels.iter().any(|l| l.contains("signed")),
            "expected signed in {labels:?}"
        );
    }

    #[tokio::test]
    async fn live_crypto_member_after_dot() {
        let cmd = resolve_vtsls_command(None).expect("vtsls");
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("examples");
        let proxy = VtslsProxy::spawn(&cmd, &["--stdio".into()], Some(root.clone()))
            .await
            .expect("spawn");
        // Real path under examples/ (same folder as scripts/ + jsconfig) — not a fake URI
        let http = r#"### POST with crypto
{{
  const crypto = require('crypto');
  crypto.
}}
POST https://example.com
"#;
        let uri = Url::from_file_path(root.join("environments.http")).unwrap();
        proxy.sync_document(&uri, http).await.expect("sync");
        let shadow = shadow_js_path(&uri).expect("shadow");
        assert!(shadow.is_file(), "missing shadow {}", shadow.display());
        let line = http
            .lines()
            .position(|l| l.trim().starts_with("crypto."))
            .expect("crypto. line") as u32;
        let col = http.lines().nth(line as usize).map(|l| l.len() as u32).unwrap_or(9);
        let pos = Position {
            line,
            character: col,
        };
        tokio::time::sleep(std::time::Duration::from_millis(2500)).await;
        let res = proxy.completion(&uri, pos, None).await.expect("ok");
        let items: Vec<CompletionItem> = match res {
            Some(CompletionResponse::Array(items)) => items,
            Some(CompletionResponse::List(l)) => l.items,
            None => vec![],
        };
        let labels: Vec<String> = items.iter().map(|i| i.label.clone()).collect();
        eprintln!(
            "shadow={} n={} first40={:?}",
            shadow.display(),
            labels.len(),
            &labels[..labels.len().min(40)]
        );
        assert!(
            labels.iter().any(|l| {
                l.contains("createHmac") || l.contains("createHash") || l.contains("randomBytes")
            }),
            "expected Node crypto members like auth-sign.js, got {labels:?}"
        );
        // .http cursor line must match textEdit line (1:1 mapping)
        for it in items.iter().filter(|i| {
            i.label.contains("createHmac") || i.label.contains("createHash")
        }) {
            if let Some(CompletionTextEdit::Edit(edit)) = &it.text_edit {
                assert_eq!(
                    edit.range.start.line, line,
                    "crypto member textEdit line must be {line}, got {:?}",
                    edit.range
                );
            }
            eprintln!(
                "ITEM {} insert={:?} format={:?} text_edit={:?} detail={:?}",
                it.label,
                it.insert_text,
                it.insert_text_format,
                it.text_edit.as_ref().map(|e| match e {
                    CompletionTextEdit::Edit(t) => t.new_text.clone(),
                    CompletionTextEdit::InsertAndReplace(ir) => ir.new_text.clone(),
                }),
                it.detail
            );
        }
        // completeFunctionCalls:true → accept should insert createHmac(...) not bare name
        let hmac = items.iter().find(|i| i.label == "createHmac" || i.label.starts_with("createHmac"));
        if let Some(it) = hmac {
            let resolved = proxy.completion_resolve(it.clone()).await.expect("resolve");
            eprintln!(
                "RESOLVED createHmac insert={:?} format={:?} text_edit={:?} detail={:?} kind={:?}",
                resolved.insert_text,
                resolved.insert_text_format,
                resolved.text_edit.as_ref().map(|e| match e {
                    CompletionTextEdit::Edit(t) => t.new_text.clone(),
                    CompletionTextEdit::InsertAndReplace(ir) => ir.new_text.clone(),
                }),
                resolved.detail,
                resolved.kind
            );
            let inserted = resolved
                .insert_text
                .clone()
                .or_else(|| {
                    resolved.text_edit.as_ref().map(|e| match e {
                        CompletionTextEdit::Edit(t) => t.new_text.clone(),
                        CompletionTextEdit::InsertAndReplace(ir) => ir.new_text.clone(),
                    })
                })
                .unwrap_or_else(|| resolved.label.clone());
            eprintln!("createHmac final insert = {inserted:?}");
            assert!(
                inserted.contains('('),
                "accept must insert call with params like JS, got {inserted:?}"
            );
        }
    }

    /// Parity with auth-sign.js: after require destructure, typing `signRe` → signRequest.
    #[tokio::test]
    async fn live_signre_from_require_like_js() {
        let cmd = resolve_vtsls_command(None).expect("vtsls");
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("examples");
        let proxy = VtslsProxy::spawn(&cmd, &["--stdio".into()], Some(root.clone()))
            .await
            .expect("spawn");
        let http = r#"### Signed
{{
  const { signRequest } = require('./scripts/auth-sign.js');
  signRe
}}
"#;
        let uri = Url::from_file_path(root.join("script-vtsls.http")).unwrap();
        proxy.sync_document(&uri, http).await.expect("sync");
        let shadow = shadow_js_path(&uri).unwrap();
        eprintln!("shadow exists={} path={}", shadow.is_file(), shadow.display());
        eprintln!("shadow head:\n{}", fs::read_to_string(&shadow).unwrap().chars().take(500).collect::<String>());
        let line = http.lines().position(|l| l.contains("signRe")).unwrap() as u32;
        let col = http.lines().nth(line as usize).unwrap().len() as u32;
        let pos = Position { line, character: col };
        tokio::time::sleep(std::time::Duration::from_millis(3000)).await;
        let res = proxy.completion(&uri, pos, None).await.expect("completion");
        let items: Vec<CompletionItem> = match res {
            Some(CompletionResponse::Array(i)) => i,
            Some(CompletionResponse::List(l)) => l.items,
            None => vec![],
        };
        let labels: Vec<String> = items.iter().map(|x| x.label.clone()).collect();
        eprintln!(
            "n={} labels(prefix sign)={:?}",
            labels.len(),
            labels
                .iter()
                .filter(|l| l.to_lowercase().contains("sign"))
                .collect::<Vec<_>>()
        );
        assert!(
            labels.iter().any(|l| l.contains("signRequest")),
            "like auth-sign.js, signRe must complete to signRequest; got {labels:?}"
        );
        // Position mapping: textEdit must land on the same .http line as the cursor
        for it in items.iter().filter(|i| i.label.contains("signRequest")) {
            if let Some(CompletionTextEdit::Edit(edit)) = &it.text_edit {
                assert_eq!(
                    edit.range.start.line, line,
                    "textEdit must target .http cursor line {line}, got {:?}",
                    edit.range
                );
            }
        }
    }

    #[tokio::test]
    async fn live_hmac_chain_after_createhmac() {
        let cmd = resolve_vtsls_command(None).expect("vtsls");
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).parent().unwrap().join("examples");
        let proxy = VtslsProxy::spawn(&cmd, &["--stdio".into()], Some(root.clone())).await.expect("spawn");
        let http = r#"###
{{
  const crypto = require('crypto');
  const h = crypto.createHmac('sha256', 'secret');
  h.
}}
"#;
        let uri = Url::from_file_path(root.join("script-vtsls.http")).unwrap();
        proxy.sync_document(&uri, http).await.unwrap();
        let pos = Position { line: 4, character: 4 }; // "  h."
        tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
        let res = proxy.completion(&uri, pos, None).await.unwrap();
        let labels: Vec<_> = match res {
            Some(CompletionResponse::Array(i)) => i.into_iter().map(|x| x.label).collect(),
            Some(CompletionResponse::List(l)) => l.items.into_iter().map(|x| x.label).collect(),
            None => vec![],
        };
        eprintln!("hmac members={labels:?}");
        assert!(labels.iter().any(|l| l.contains("update") || l.contains("digest")), "{labels:?}");
    }

    /// Zed query for `crypto.` is the string "crypto." — raw vtsls labels like
    /// "createHmac" get client-filtered out. Catalog uses filter_text = "crypto.createHmac".
    /// This test drives the same rewrite main.rs applies and asserts Zed would keep items.
    #[test]
    fn zed_filter_text_keeps_crypto_dot_members() {
        // Simulate what completion_inner does after vtsls returns bare labels
        let before = "  crypto.";
        let (path, partial) = crate::completions::dotted_prefix(before).unwrap_or_default();
        assert_eq!(path, "crypto");
        assert_eq!(partial, "");
        let bare_labels = ["createHmac", "createHash", "randomBytes"];
        let mut kept = 0;
        for name in bare_labels {
            let (ft, _lab) = crate::completions::member_filter_label(&path, name);
            // Zed fuzzy-matches query "crypto." against filter_text
            let query = "crypto.";
            let matches = ft.to_lowercase().contains(&query.to_lowercase().trim_end_matches('.').to_string())
                || ft.starts_with("crypto.")
                || ft.contains(query);
            // Our filter_text is "crypto.createHmac" which starts with "crypto."
            assert!(
                ft.starts_with("crypto."),
                "filter_text must be path.name for Zed, got {ft}"
            );
            assert!(
                ft.contains(name),
                "filter_text must contain member name"
            );
            if ft.starts_with("crypto.") {
                kept += 1;
            }
            let _ = matches;
        }
        assert_eq!(kept, 3, "all three members must survive Zed-style filter");
    }

    #[tokio::test]
    async fn real_environments_http_crypto_completion() {
        let cmd = resolve_vtsls_command(None).expect("vtsls");
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("examples");
        let proxy = VtslsProxy::spawn(&cmd, &["--stdio".into()], Some(root.clone()))
            .await
            .expect("spawn");

        // Exact user buffer shape (crypto. after require) — not relying on disk dirty state
        let http = "### POST with auth_token from env\n{{\n  //pre request script\n  const crypto = require('crypto');\n  crypto.\n  const date = new Date();\n}}\nPOST https://example.com\n";
        let uri = Url::from_file_path(root.join("environments.http")).expect("uri");
        proxy.sync_document(&uri, http).await.expect("sync");

        let shadow = shadow_js_path(&uri).expect("shadow path");
        let shadow_txt = fs::read_to_string(&shadow).expect("read shadow");
        assert!(
            shadow_txt.contains("require('crypto')"),
            "shadow missing require crypto; content:\n{shadow_txt}"
        );
        assert!(
            shadow_txt.lines().any(|l| l.trim().starts_with("crypto.")),
            "shadow missing crypto. line; content:\n{shadow_txt}"
        );
        assert!(
            !shadow_txt.contains("http-client.env"),
            "shadow polluted by HTTP comment: {shadow_txt}"
        );

        let line = http
            .lines()
            .position(|l| l.trim().starts_with("crypto."))
            .expect("crypto. line") as u32;
        let col = http.lines().nth(line as usize).map(|l| l.len() as u32).unwrap_or(9);
        let pos = Position {
            line,
            character: col,
        };
        eprintln!("crypto. at line={line} col={col}");
        tokio::time::sleep(std::time::Duration::from_millis(3000)).await;
        let res = proxy.completion(&uri, pos, None).await.expect("completion");
        let items: Vec<CompletionItem> = match res {
            Some(CompletionResponse::Array(i)) => i,
            Some(CompletionResponse::List(l)) => l.items,
            None => vec![],
        };
        let labels: Vec<String> = items.iter().map(|x| x.label.clone()).collect();
        eprintln!(
            "n={} first30={:?}",
            labels.len(),
            &labels[..labels.len().min(30)]
        );
        assert!(
            labels.iter().any(|l| {
                l.contains("createHmac") || l.contains("createHash") || l.contains("randomBytes")
            }),
            "crypto. must complete via vtsls+@types/node; got {labels:?}"
        );
        // Apply same filter_text rewrite as main.rs completion_inner
        let before = http.lines().nth(line as usize).unwrap();
        let (path, _) = crate::completions::dotted_prefix(before).unwrap_or_default();
        assert_eq!(path, "crypto", "dotted_prefix must see crypto.");
        for it in &items {
            if it.label.contains("createHmac") || it.label.contains("createHash") {
                let (ft, _) = crate::completions::member_filter_label(&path, &it.label);
                assert!(
                    ft.starts_with("crypto."),
                    "Zed filter_text must be crypto.NAME, got {ft}"
                );
            }
        }
    }

    /// Generic require('fs') — proves no crypto hardcode; any Node module works via @types/node
    #[tokio::test]
    async fn real_require_fs_completion() {
        let cmd = resolve_vtsls_command(None).expect("vtsls");
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("examples");
        let proxy = VtslsProxy::spawn(&cmd, &["--stdio".into()], Some(root.clone()))
            .await
            .expect("spawn");

        let http = "### fs\n{{\n  const fs = require('fs');\n  fs.\n}}\nPOST https://example.com\n";
        let uri = Url::from_file_path(root.join("_fs_vtsls_test.http")).unwrap();
        proxy.sync_document(&uri, http).await.expect("sync");

        let shadow = shadow_js_path(&uri).unwrap();
        let st = fs::read_to_string(&shadow).unwrap();
        assert!(st.contains("require('fs')"), "shadow missing fs require: {st}");
        assert!(st.lines().any(|l| l.trim().starts_with("fs.")), "shadow missing fs. line");

        let line = http.lines().position(|l| l.trim().starts_with("fs.")).unwrap() as u32;
        let col = http.lines().nth(line as usize).unwrap().len() as u32;
        let pos = Position { line, character: col };
        tokio::time::sleep(std::time::Duration::from_millis(3000)).await;
        let res = proxy.completion(&uri, pos, None).await.expect("ok");
        let items: Vec<CompletionItem> = match res {
            Some(CompletionResponse::Array(i)) => i,
            Some(CompletionResponse::List(l)) => l.items,
            None => vec![],
        };
        let labels: Vec<String> = items.iter().map(|x| x.label.clone()).collect();
        eprintln!("fs. n={} sample={:?}", labels.len(), &labels[..labels.len().min(20)]);
        assert!(
            labels.iter().any(|l| l.contains("readFileSync") || l.contains("writeFileSync") || l.contains("existsSync")),
            "fs. must complete via pure vtsls+@types/node (no hardcode); got {labels:?}"
        );
        // Generic filter_text path (not crypto-specific)
        let (path, _) = crate::completions::dotted_prefix("  fs.").unwrap_or_default();
        assert_eq!(path, "fs");
        let (ft, _) = crate::completions::member_filter_label(&path, "readFileSync");
        assert_eq!(ft, "fs.readFileSync");
    }


}
