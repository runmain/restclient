//! Child **vtsls** process for script islands inside `.http` buffers.
//!
//! This is **not** Zed-managed vtsls (that only attaches to real JS/TS buffers).
//! httpyac-lsp spawns `@vtsls/language-server` (`vtsls --stdio`), feeds a
//! **line-aligned virtual `.ts` document**, and forwards completion / hover /
//! definition so script regions get full TypeScript IntelliSense.
//!
//! Configure with (priority high → low):
//! 1. `lsp.httpyac-lsp.settings.vtslsCommand`
//! 2. env `HTTPYAC_VTSLS_COMMAND`
//! 3. `PATH` → `vtsls`
//!
//! Disable: `settings.vtslsEnabled: false` or missing binary (falls back to catalog).

use std::collections::HashMap;
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

/// Ambient lines prepended to every virtual TS doc (httpyac script globals).
pub const AMBIENT_PREAMBLE: &str = r#"/* httpyac-lsp virtual buffer — do not edit */
/** httpyac request object */
declare const request: {
  method: string;
  url: string;
  headers: Record<string, string> & { set?(k: string, v: string): void; get?(k: string): string };
  body?: unknown;
  [key: string]: unknown;
};
/** httpyac response object */
declare const response: {
  statusCode: number;
  status: number;
  headers: Record<string, string>;
  body: string | unknown;
  parsedBody?: unknown;
  contentType?: string;
  [key: string]: unknown;
};
declare const client: {
  test(name: string, fn: () => void): void;
  assert(cond: unknown, message?: string): void;
  log(...args: unknown[]): void;
  global: { get(k: string): unknown; set(k: string, v: unknown): void; clear(k?: string): void };
  [key: string]: unknown;
};
declare const console: Console;
declare const exports: Record<string, unknown>;
declare function test(name: string, fn: () => void): void;
declare function require(id: string): any;
export {};
"#;

/// User / init settings for the child vtsls.
#[derive(Debug, Clone)]
pub struct VtslsSettings {
    pub enabled: bool,
    /// Absolute or PATH command, e.g. `/…/bin/vtsls`.
    pub command: Option<String>,
    pub args: Vec<String>,
}

impl Default for VtslsSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            command: None,
            args: vec!["--stdio".into()],
        }
    }
}

impl VtslsSettings {
    /// Merge JSON from `initializationOptions` or `lsp.httpyac-lsp.settings`.
    pub fn from_json(v: &Value) -> Self {
        let mut s = Self::default();
        if let Some(b) = v.get("vtslsEnabled").and_then(|x| x.as_bool()) {
            s.enabled = b;
        }
        // camelCase + snake_case
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
        // enabled: explicit false wins if other set from json with false
        self.enabled = other.enabled;
        if other.args != vec!["--stdio".to_string()] || self.args.is_empty() {
            if !other.args.is_empty() {
                self.args = other.args.clone();
            }
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

/// Line-aligned virtual TS: preamble + one output line per HTTP line.
/// Script interior keeps source text; fences / HTTP lines become blank.
pub fn build_virtual_typescript(http_src: &str) -> (String, u32) {
    let preamble_lines = AMBIENT_PREAMBLE.lines().count() as u32;
    let mut out = String::from(AMBIENT_PREAMBLE);
    if !out.ends_with('\n') {
        out.push('\n');
    }

    let mut in_mustache = false; // {{ … }}
    let mut in_percent = false; // {% … %}

    for line in http_src.lines() {
        let trim = line.trim();
        let mut emit = String::new();

        // Toggle / single-line percent handlers: > {% … %}  or < {% … %}
        if trim.contains("{%") {
            in_percent = true;
        }
        if trim.contains("%}") {
            // content before %} on same line is rare; treat whole fence line as non-JS
            if in_percent {
                in_percent = false;
                out.push('\n');
                continue;
            }
        }

        if trim == "{{" || trim.starts_with("{{") && trim.ends_with("}}") && trim.len() > 4 {
            // single-line {{ code }}
            if trim.starts_with("{{") && trim.ends_with("}}") && !trim[2..].contains("{{") {
                let inner = trim.trim_start_matches("{{").trim_end_matches("}}").trim();
                emit = inner.to_string();
            } else if trim == "{{" || trim.starts_with("{{") && !trim.contains("}}") {
                in_mustache = true;
            }
        } else if trim == "}}" || (trim.ends_with("}}") && in_mustache) {
            in_mustache = false;
        } else if in_mustache || in_percent {
            // Strip leading `>` from handler lines inside block
            let body = line.strip_prefix('>').unwrap_or(line);
            let body = body.strip_prefix('<').unwrap_or(body);
            emit = body.to_string();
            // Drop pure fence leftovers
            if emit.trim() == "{%" || emit.trim() == "%}" || emit.trim() == "{{" || emit.trim() == "}}"
            {
                emit.clear();
            }
        } else if looks_like_inline_script_line(trim) {
            // Loose JS signals outside formal blocks (same as completions heuristic)
            emit = line.to_string();
        }

        out.push_str(&emit);
        out.push('\n');
    }

    (out, preamble_lines)
}

fn looks_like_inline_script_line(trim: &str) -> bool {
    const SIG: &[&str] = &[
        "const ", "let ", "var ", "function ", "await ", "async ", "require(", "exports.",
        "response.", "request.", "client.", "console.", "crypto.", "return ",
    ];
    SIG.iter().any(|s| trim.contains(s))
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

fn virtual_uri_for(http_uri: &Url) -> Url {
    let s = http_uri.as_str();
    if s.ends_with(".httpyac.ts") {
        return http_uri.clone();
    }
    Url::parse(&format!("{s}.httpyac.ts")).unwrap_or_else(|_| http_uri.clone())
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
        let pending_r = pending.clone();

        // Read stdout loop
        tokio::spawn(async move {
            if let Err(e) = read_lsp_stdout(stdout, pending_r).await {
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
            stdin: Arc::new(Mutex::new(stdin)),
            pending,
            next_id: AtomicU64::new(1),
            versions: Mutex::new(HashMap::new()),
            preamble_lines: AMBIENT_PREAMBLE.lines().count() as u32,
            command_path: command.to_string(),
        };

        proxy.initialize(workspace_root).await?;
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

        match tokio::time::timeout(std::time::Duration::from_secs(8), rx).await {
            Ok(Ok(Ok(v))) => Ok(v),
            Ok(Ok(Err(e))) => Err(e),
            Ok(Err(_)) => Err("vtsls response channel closed".into()),
            Err(_) => {
                let mut p = self.pending.lock().await;
                p.map.remove(&id);
                Err("vtsls request timed out".into())
            }
        }
    }

    /// Sync virtual TS document for an open `.http` buffer.
    pub async fn sync_document(&self, http_uri: &Url, http_text: &str) -> Result<(), String> {
        let (virtual_text, preamble) = build_virtual_typescript(http_text);
        let _ = preamble; // stored on struct from ambient constant
        let vuri = virtual_uri_for(http_uri);
        let key = vuri.to_string();
        let mut versions = self.versions.lock().await;
        let is_new = !versions.contains_key(&key);
        let ver = versions.entry(key.clone()).or_insert(0);
        *ver += 1;
        let version = *ver;
        drop(versions);

        if is_new {
            self.notify(
                "textDocument/didOpen",
                json!({
                    "textDocument": {
                        "uri": vuri,
                        "languageId": "typescript",
                        "version": version,
                        "text": virtual_text,
                    }
                }),
            )
            .await?;
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
        self.notify(
            "textDocument/didClose",
            json!({ "textDocument": { "uri": vuri } }),
        )
        .await
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
                // Mark source for debugging / sort
                if it.detail.is_none() {
                    it.detail = Some("vtsls".into());
                } else if let Some(d) = it.detail.as_mut() {
                    if !d.contains("vtsls") {
                        *d = format!("{d} · vtsls");
                    }
                }
                if it.sort_text.is_none() {
                    it.sort_text = Some(format!("0{}", it.label));
                }
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
            if it.detail.is_none() {
                it.detail = Some("vtsls".into());
            }
        }
        if list.items.is_empty() {
            return None;
        }
        return Some(CompletionResponse::List(list));
    }
    None
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

async fn read_lsp_stdout(
    stdout: tokio::process::ChildStdout,
    pending: Arc<Mutex<Pending>>,
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
        let len = content_length.ok_or("vtsls message missing Content-Length")?;
        let mut buf = vec![0u8; len];
        reader
            .read_exact(&mut buf)
            .await
            .map_err(|e| e.to_string())?;
        let msg: Value = serde_json::from_slice(&buf).map_err(|e| e.to_string())?;

        if let Some(id) = msg.get("id") {
            // response
            let id_n = id.as_u64().or_else(|| id.as_i64().map(|i| i as u64));
            if let Some(id_n) = id_n {
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
        }
        // ignore server → client requests/notifications for now (capabilities minimal)
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

    pub async fn ensure_started(
        &self,
        client: &Client,
        workspace_root: Option<PathBuf>,
    ) -> Option<()> {
        {
            let s = self.settings.lock().await;
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
        assert!(preamble > 5);
        // HTTP line count == non-preamble virtual lines
        let http_lines = src.lines().count();
        let virt_body_lines = virt.lines().count() - preamble as usize;
        assert_eq!(virt_body_lines, http_lines);
        assert!(virt.contains("createHmac"));
        assert!(!virt.contains("GET https://"));
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
    }
}
