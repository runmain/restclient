use httpyac_lsp::completions::{
    dotted_prefix, header_values, in_script_context, ARRAY_PROPS, AUTH_SCHEMES, BUILTIN_VARS,
    CLIENT_GLOBAL_PROPS, CLIENT_PROPS, DATE_PROPS, HEADER_NAMES, HTTP_METHODS, JSON_PROPS,
    MATH_PROPS, META_DIRECTIVES, OBJECT_PROPS, REQUEST_PROPS, REQUIRE_MODULES, RESPONSE_HEADERS_PROPS,
    RESPONSE_PROPS, SCRIPT_ROOTS, SCRIPT_SNIPPETS,
};
use httpyac_lsp::parser::{parse_http_file, HttpRequest};
use httpyac_lsp::variables::VariableResolver;
use httpyac_lsp::{collect_env_names, resolve_httpyac_bin};

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};

// Must match Zed's lsp_ext_command.rs Runnable/ShellRunnableArgs structs exactly
//
// Zed gutter 实际展示的是 languages/http/tasks.json（标签 http-request），
// 不是 LSP experimental/runnables。因此 env 选择主要靠 tasks + httpyac-run。
// LSP runnables 仍提供带 --env 的按钮（部分 Zed 版本会显示）。
//
// 执行链路：httpyac-run → httpyac send --line N [--env name]
// 变量 / script / HTTP 全部由 httpyac 完成。

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct RunnablesParams {
    text_document: TextDocumentIdentifier,
    _position: Option<Position>,
}

#[derive(Debug, serde::Serialize)]
struct Runnable {
    label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    location: Option<LocationLink>,
    kind: &'static str,
    args: ShellRunnableArgs,
}

#[derive(Debug, serde::Serialize)]
struct ShellRunnableArgs {
    program: String,
    args: Vec<String>,
    cwd: String,
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    environment: HashMap<String, String>,
}

#[derive(Debug)]
struct Document {
    content: String,
    version: i32,
    requests: Vec<HttpRequest>,
}

#[derive(Debug)]
struct HttpLsp {
    client: Client,
    documents: Arc<RwLock<HashMap<Url, Document>>>,
}

impl HttpLsp {
    fn new(client: Client) -> Self {
        HttpLsp {
            client,
            documents: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Load env vars for `{{…}}` completion from http-client.env.json (all envs).
    /// Returns (name, value, source_detail).
    fn load_env_vars_for_completion(uri: &Url) -> Vec<(String, String, String)> {
        let file_path = match uri.to_file_path() {
            Ok(p) => p,
            Err(_) => return Vec::new(),
        };
        let dir = file_path
            .parent()
            .unwrap_or_else(|| std::path::Path::new("."));
        let mut resolver = VariableResolver::new();
        if !resolver.load_environments_from_dir(dir) {
            return Vec::new();
        }
        resolver.get_completion_env_variables()
    }

    async fn load_document_for_uri(&self, uri: &Url) -> Option<Document> {
        let docs = self.documents.read().await;
        if let Some(doc) = docs.get(uri) {
            return Some(Document {
                content: doc.content.clone(),
                version: doc.version,
                requests: doc.requests.clone(),
            });
        }
        drop(docs);

        let file_path = uri.to_file_path().ok()?;
        let content = std::fs::read_to_string(file_path).ok()?;
        let requests = parse_http_file(&content).ok()?;

        Some(Document {
            content,
            version: 0,
            requests,
        })
    }

    fn make_location(uri: &Url, start_line: u32, end_line: u32) -> LocationLink {
        LocationLink {
            origin_selection_range: None,
            target_uri: uri.clone(),
            target_range: Range {
                start: Position {
                    line: start_line,
                    character: 0,
                },
                end: Position {
                    line: end_line,
                    character: 0,
                },
            },
            target_selection_range: Range {
                start: Position {
                    line: start_line,
                    character: 0,
                },
                end: Position {
                    line: start_line,
                    character: 100,
                },
            },
        }
    }

    /// Shell runnable via httpyac-run → httpyac (never bare httpyac without env handling).
    fn make_send_runnable(
        label: String,
        location: LocationLink,
        runner: &str,
        file_path: &str,
        line: usize,
        cwd: &str,
        env_name: Option<&str>,
    ) -> Runnable {
        let mut args = vec![file_path.to_string(), line.to_string()];
        if let Some(env) = env_name {
            args.push("--env".to_string());
            args.push(env.to_string());
        }
        Runnable {
            label,
            location: Some(location),
            kind: "shell",
            args: ShellRunnableArgs {
                program: runner.to_string(),
                args,
                cwd: cwd.to_string(),
                environment: HashMap::new(),
            },
        }
    }

    async fn handle_runnables(&self, params: RunnablesParams) -> Result<Vec<Runnable>> {
        let uri = params.text_document.uri;

        let doc = match self.load_document_for_uri(&uri).await {
            Some(d) => d,
            None => return Ok(Vec::new()),
        };

        if doc.requests.is_empty() {
            return Ok(Vec::new());
        }

        let file_path = uri
            .to_file_path()
            .unwrap_or_else(|_| PathBuf::from(uri.path()));

        let file_path_str = file_path.to_string_lossy().to_string();

        let cwd = file_path
            .parent()
            .unwrap_or(&file_path)
            .to_string_lossy()
            .to_string();

        let file_dir = file_path
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| PathBuf::from("."));

        let (active_env, env_names) = collect_env_names(&file_dir);
        // Prefer httpyac-run on PATH (installed by install_to_zed.sh)
        let runner = resolve_runner_bin();

        let mut runnables = Vec::new();

        for request in &doc.requests {
            let method_str = request.method.as_str();
            let start_line = request.metadata.start_line.saturating_sub(1) as u32;
            let end_line = request.metadata.end_line.saturating_sub(1) as u32;
            let line = request.metadata.start_line;

            let default_label = match &active_env {
                Some(env) => format!("▶ Send [{}] {} {}", env, method_str, request.url),
                None => format!("▶ Send {} {}", method_str, request.url),
            };
            runnables.push(Self::make_send_runnable(
                default_label,
                Self::make_location(&uri, start_line, end_line),
                &runner,
                &file_path_str,
                line,
                &cwd,
                active_env.as_deref(),
            ));

            // Interactive env picker (shows in terminal)
            if !env_names.is_empty() {
                runnables.push(Runnable {
                    label: format!("🌐 选择环境… {} {}", method_str, request.url),
                    location: Some(Self::make_location(&uri, start_line, end_line)),
                    kind: "shell",
                    args: ShellRunnableArgs {
                        program: runner.clone(),
                        args: vec![
                            file_path_str.clone(),
                            line.to_string(),
                            "--pick-env".to_string(),
                        ],
                        cwd: cwd.clone(),
                        environment: HashMap::new(),
                    },
                });
            }

            for env_name in &env_names {
                if active_env.as_ref() == Some(env_name) {
                    continue;
                }
                let label = format!("🌐 Env:{} {} {}", env_name, method_str, request.url);
                runnables.push(Self::make_send_runnable(
                    label,
                    Self::make_location(&uri, start_line, end_line),
                    &runner,
                    &file_path_str,
                    line,
                    &cwd,
                    Some(env_name.as_str()),
                ));
            }
        }

        Ok(runnables)
    }
}

/// Detect cursor inside / starting a `{{ … }}` for variable completion.
/// Returns (replace_start_character, partial name, replace_end_extra after cursor).
///
/// Examples (cursor at |):
/// - `{{|` / `{{|}}`     → partial ""
/// - `{{b|` / `{{b|}}`   → partial "b"
/// - `{|` / `{|}`        → partial "" (Zed brace-autoclose first `{`)
/// - `{{base_url}}|`     → None (already closed before cursor)
fn mustache_context(before: &str, after: &str, _char_pos: u32) -> Option<(u32, String, u32)> {
    // Prefer real `{{ …` open
    if let Some(open) = before.rfind("{{") {
        let between = &before[open + 2..];
        if between.contains("}}") {
            return None;
        }
        let partial: String = between
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '$' || *c == '.')
            .collect();
        // Consume auto-closed `}}` or `}` after cursor so replace doesn't leave extras
        let extra = mustache_closing_extra(after);
        return Some((open as u32, partial, extra));
    }

    // Single `{` — common with Zed `brackets` auto-close (`{|}` while typing `{{`)
    if before.ends_with('{') && !before.ends_with("\\{") {
        let open = before.len() - 1;
        let extra = mustache_closing_extra(after);
        return Some((open as u32, String::new(), extra));
    }

    let _ = after;
    None
}

/// How many chars after the cursor belong to a brace-pair close we should replace.
fn mustache_closing_extra(after: &str) -> u32 {
    if after.starts_with("}}") {
        2
    } else if after.starts_with('}') {
        1
    } else {
        0
    }
}

/// If cursor is inside `require('…` or `require("…`, return the partial module name.
fn require_module_partial(before: &str) -> Option<String> {
    let lower = before.to_ascii_lowercase();
    let idx = lower.rfind("require(")?;
    let after = &before[idx + "require(".len()..];
    let after = after.trim_start();
    let (quote, rest) = if let Some(r) = after.strip_prefix('\'') {
        ('\'', r)
    } else if let Some(r) = after.strip_prefix('"') {
        ('"', r)
    } else {
        return None;
    };
    if rest.contains(quote) {
        return None; // already closed
    }
    let partial: String = rest
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-' || *c == '/' || *c == '.')
        .collect();
    Some(partial)
}

fn resolve_runner_bin() -> String {
    if let Ok(p) = std::env::var("HTTPYAC_RUN_BIN") {
        if !p.is_empty() {
            return p;
        }
    }
    let which = if cfg!(windows) { "where" } else { "which" };
    if let Ok(out) = std::process::Command::new(which).arg("httpyac-run").output() {
        if out.status.success() {
            let p = String::from_utf8_lossy(&out.stdout)
                .lines()
                .next()
                .unwrap_or("")
                .trim()
                .to_string();
            if !p.is_empty() {
                return p;
            }
        }
    }
    let home = std::env::var("HOME").unwrap_or_default();
    let local = format!("{home}/.local/bin/httpyac-run");
    if std::path::Path::new(&local).is_file() {
        return local;
    }
    // last resort: still try PATH name
    let _ = resolve_httpyac_bin();
    "httpyac-run".to_string()
}

#[tower_lsp::async_trait]
impl LanguageServer for HttpLsp {
    async fn initialize(&self, _params: InitializeParams) -> Result<InitializeResult> {
        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                document_symbol_provider: Some(OneOf::Left(true)),
                completion_provider: Some(CompletionOptions {
                    resolve_provider: Some(false),
                    // Include . @ $ > for httpyac script / meta / variables
                    trigger_characters: Some(
                        "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ :/.@${}>"
                            .chars()
                            .map(|c| c.to_string())
                            .collect(),
                    ),
                    ..Default::default()
                }),
                ..Default::default()
            },
            ..Default::default()
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        self.client
            .log_message(MessageType::INFO, "HTTP LSP initialized")
            .await;
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let uri = params.text_document.uri;
        let content = params.text_document.text;
        let version = params.text_document.version;

        let requests = parse_http_file(&content).unwrap_or_default();
        let doc = Document {
            content,
            version,
            requests,
        };
        let mut docs = self.documents.write().await;
        docs.insert(uri, doc);
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri;
        let version = params.text_document.version;

        if let Some(change) = params.content_changes.into_iter().last() {
            let requests = parse_http_file(&change.text).unwrap_or_default();
            let doc = Document {
                content: change.text,
                version,
                requests,
            };
            let mut docs = self.documents.write().await;
            docs.insert(uri, doc);
        }
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        let mut docs = self.documents.write().await;
        docs.remove(&params.text_document.uri);
    }

    async fn document_symbol(
        &self,
        params: DocumentSymbolParams,
    ) -> Result<Option<DocumentSymbolResponse>> {
        let uri = params.text_document.uri;

        let docs = self.documents.read().await;

        let requests = if let Some(doc) = docs.get(&uri) {
            doc.requests.clone()
        } else {
            if let Ok(file_path) = uri.to_file_path() {
                if let Ok(content) = std::fs::read_to_string(&file_path) {
                    parse_http_file(&content).unwrap_or_default()
                } else {
                    Vec::new()
                }
            } else {
                Vec::new()
            }
        };

        if requests.is_empty() {
            return Ok(None);
        }

        let mut symbols = Vec::new();

        for request in &requests {
            let start_line = request.metadata.start_line.saturating_sub(1) as u32;
            let end_line = request.metadata.end_line.saturating_sub(1) as u32;

            #[allow(deprecated)]
            let symbol = DocumentSymbol {
                name: format!("{:?} {}", request.method, request.url),
                detail: Some("HTTP Request".to_string()),
                kind: SymbolKind::METHOD,
                tags: None,
                deprecated: None,
                range: Range {
                    start: Position {
                        line: start_line,
                        character: 0,
                    },
                    end: Position {
                        line: end_line,
                        character: 0,
                    },
                },
                selection_range: Range {
                    start: Position {
                        line: start_line,
                        character: 0,
                    },
                    end: Position {
                        line: start_line,
                        character: 100,
                    },
                },
                children: None,
            };

            symbols.push(symbol);
        }

        Ok(Some(DocumentSymbolResponse::Nested(symbols)))
    }

    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        let uri = params.text_document_position.text_document.uri;
        let position = params.text_document_position.position;

        // Prefer in-memory buffer; fall back to disk (unsynced / not yet did_open)
        let content = {
            let docs = self.documents.read().await;
            if let Some(d) = docs.get(&uri) {
                d.content.clone()
            } else if let Ok(path) = uri.to_file_path() {
                std::fs::read_to_string(path).unwrap_or_default()
            } else {
                String::new()
            }
        };
        if content.is_empty() {
            return Ok(None);
        }

        let lines: Vec<&str> = content.split('\n').collect();

        if position.line as usize >= lines.len() {
            return Ok(None);
        }

        let line_idx = position.line as usize;
        let current_line = lines[line_idx];
        // Zed may send UTF-16 offsets; for ASCII .http this matches bytes. Clamp safely.
        let char_pos = (position.character as usize).min(current_line.len());
        let before_cursor = &current_line[..char_pos];
        let after_cursor = &current_line[char_pos..];
        let trimmed = before_cursor.trim();
        let full_line_trimmed = current_line.trim();
        let line_num = position.line;
        let line_start_char = (current_line.len() - current_line.trim_start().len()) as u32;

        let mut items = Vec::new();

        // ### separators: no completions
        if full_line_trimmed.starts_with("###") && !before_cursor.contains("{{") {
            return Ok(None);
        }

        // ── {{ variable }} completion — HIGHEST priority (URL / header / body / script) ──
        // Same behavior everywhere mustache appears: GET {{…}}, Host: {{…}}, JSON body,
        // @var = {{…}}, script strings, bare line, etc.
        //
        // Zed client-filters completions with a *query* derived from word/query chars.
        // languages/http/config.toml marks `{` `}` as word_characters so the query is
        // always the mustache token (`{{` / `{{b`), never `Host: {{` or `"user": "{{`.
        // filter_text/label stay mustache-shaped so URL/header/body/script all match.
        if let Some((var_start, partial, close_extra)) =
            mustache_context(before_cursor, after_cursor, position.character)
        {
            let partial_l = partial.to_lowercase();
            let mut seen = std::collections::HashSet::new();
            let end_char = position.character + close_extra;
            // Full line text through cursor — belt-and-suspenders if Zed still sends a
            // long query (older config without word_characters fix).
            let typed_through = before_cursor.to_string();
            let line_prefix = before_cursor
                .get(..var_start as usize)
                .unwrap_or("")
                .to_string();

            let push_var = |items: &mut Vec<CompletionItem>,
                            var_name: &str,
                            detail: String,
                            kind: CompletionItemKind,
                            sort_prefix: &str,
                            docs: Option<String>| {
                let insert = format!("{{{{{var_name}}}}}");
                // Single primary filter string (mustache-local) + fallbacks without
                // relying on space-splitting (Zed fuzzy-matches the whole filterText).
                // Order: most common queries first for ranking.
                let filter = format!(
                    "{insert}\n{{{{{partial}{var_name}}}}}\n{var_name}\n{partial}\n{typed_through}{var_name}\n{line_prefix}{insert}"
                );
                items.push(CompletionItem {
                    label: insert.clone(),
                    kind: Some(kind),
                    detail: Some(detail),
                    // Replace only the `{`/`{{…` span (keep `Host: ` / `GET ` / JSON quotes)
                    text_edit: Some(CompletionTextEdit::Edit(TextEdit {
                        range: Range {
                            start: Position {
                                line: line_num,
                                character: var_start,
                            },
                            end: Position {
                                line: line_num,
                                character: end_char,
                            },
                        },
                        new_text: insert.clone(),
                    })),
                    insert_text: Some(insert.clone()),
                    // Also expose bare name for when query is just `b` inside `{{b|`
                    filter_text: Some(filter),
                    sort_text: Some(format!("{sort_prefix}{var_name}")),
                    documentation: docs.map(Documentation::String),
                    ..Default::default()
                });
            };

            // http-client.env.json (all environments)
            for (var_name, var_value, source) in Self::load_env_vars_for_completion(&uri) {
                if !partial_l.is_empty() && !var_name.to_lowercase().starts_with(&partial_l) {
                    continue;
                }
                seen.insert(var_name.clone());
                push_var(
                    &mut items,
                    &var_name,
                    format!("{source} = {var_value}"),
                    CompletionItemKind::VARIABLE,
                    "0",
                    Some(format!(
                        "Environment variable\n{source}\nValue: {var_value}"
                    )),
                );
            }

            // @file variables
            for line in &lines {
                let l = line.trim();
                if !l.starts_with('@') {
                    continue;
                }
                if let Some(eq_pos) = l.find('=') {
                    let var_name = l[1..eq_pos].trim();
                    if var_name.is_empty() || seen.contains(var_name) {
                        continue;
                    }
                    if !partial_l.is_empty() && !var_name.to_lowercase().starts_with(&partial_l) {
                        continue;
                    }
                    seen.insert(var_name.to_string());
                    let var_value = l[eq_pos + 1..].trim();
                    push_var(
                        &mut items,
                        var_name,
                        format!("@file = {var_value}"),
                        CompletionItemKind::VARIABLE,
                        "1",
                        None,
                    );
                }
            }

            // builtins $uuid …
            for (bname, desc) in BUILTIN_VARS {
                let bl = bname.to_lowercase();
                if !partial_l.is_empty()
                    && !bl.starts_with(&partial_l)
                    && !(partial_l.starts_with('$') && bl.starts_with(&partial_l))
                {
                    continue;
                }
                push_var(
                    &mut items,
                    bname,
                    format!("httpyac builtin — {desc}"),
                    CompletionItemKind::FUNCTION,
                    "2",
                    None,
                );
            }

            // Inside {{ }}, return only variable items (don't mix headers)
            if !items.is_empty() {
                // is_incomplete so Zed re-queries as the user types more of the name
                return Ok(Some(CompletionResponse::List(CompletionList {
                    is_incomplete: true,
                    items,
                })));
            }
            // Still inside mustache but no matches — don't fall through to Host etc.
            return Ok(None);
        }

        // --- httpyac meta: # @name / // @ref ---
        let is_meta_line = full_line_trimmed.starts_with('#') || full_line_trimmed.starts_with("//");
        if is_meta_line {
            let after_hash = full_line_trimmed
                .trim_start_matches("///")
                .trim_start_matches("//")
                .trim_start_matches('#')
                .trim_start();
            // "# @na" or "@name" or bare "@"
            let meta_prefix = if after_hash.starts_with('@') {
                after_hash
            } else if trimmed.contains('@') {
                trimmed.rsplit_once('@').map(|(_, r)| r).unwrap_or("")
            } else {
                // still offer @ directives when typing after # 
                after_hash
            };
            let filter = if meta_prefix.starts_with('@') {
                meta_prefix.to_string()
            } else if after_hash.is_empty() || !after_hash.contains('@') {
                // offer inserting @name etc.
                String::new()
            } else {
                format!("@{}", meta_prefix)
            };

            for (dir, desc) in META_DIRECTIVES {
                let label = dir.to_string();
                if filter.is_empty()
                    || label.to_lowercase().starts_with(&filter.to_lowercase())
                    || format!("@{}", filter.trim_start_matches('@'))
                        .to_lowercase()
                        .starts_with(&filter.to_lowercase())
                    || dir
                        .to_lowercase()
                        .contains(&filter.trim_start_matches('@').to_lowercase())
                {
                    let insert = if dir.ends_with("name") || *dir == "@name" {
                        format!("{} ${{1:requestName}}", dir)
                    } else if *dir == "@ref" {
                        format!("{} ${{1:otherRequest}}", dir)
                    } else if *dir == "@timeout" {
                        format!("{} ${{1:30000}}", dir)
                    } else if *dir == "@import" {
                        format!("{} ${{1:./other.http}}", dir)
                    } else {
                        (*dir).to_string()
                    };
                    // Simple text without snippet placeholders if client doesn't expand
                    let simple = match *dir {
                        "@name" => "@name ".to_string(),
                        "@ref" => "@ref ".to_string(),
                        "@import" => "@import ".to_string(),
                        "@timeout" => "@timeout ".to_string(),
                        _ => format!("{} ", dir),
                    };
                    let _ = insert;
                    items.push(CompletionItem {
                        label: label.clone(),
                        kind: Some(CompletionItemKind::KEYWORD),
                        detail: Some(format!("httpyac meta — {desc}")),
                        insert_text: Some(simple.clone()),
                        insert_text_format: Some(InsertTextFormat::PLAIN_TEXT),
                        filter_text: Some(label.clone()),
                        sort_text: Some(format!("0{label}")),
                        documentation: Some(Documentation::String(desc.to_string())),
                        ..Default::default()
                    });
                }
            }
            if !items.is_empty() {
                return Ok(Some(CompletionResponse::Array(items)));
            }
            return Ok(None);
        }

        let script_ctx = in_script_context(&lines, line_idx, before_cursor);

        // --- httpyac script: response. / request. / client. ---
        if script_ctx {
            if let Some((path, filter)) = dotted_prefix(before_cursor) {
                let filter_l = filter.to_lowercase();
                let push_props = |items: &mut Vec<CompletionItem>,
                                  props: &[(&str, &str)],
                                  sort_prefix: &str| {
                    for (name, desc) in props {
                        if filter_l.is_empty() || name.to_lowercase().starts_with(&filter_l) {
                            items.push(CompletionItem {
                                label: name.to_string(),
                                kind: Some(CompletionItemKind::PROPERTY),
                                detail: Some(format!("httpyac — {desc}")),
                                insert_text: Some(name.to_string()),
                                filter_text: Some(name.to_string()),
                                sort_text: Some(format!("{sort_prefix}{name}")),
                                documentation: Some(Documentation::String(desc.to_string())),
                                ..Default::default()
                            });
                        }
                    }
                };

                match path.as_str() {
                    "" => {
                        // bare identifier in script (httpyac APIs + common JS)
                        for (name, desc) in SCRIPT_ROOTS {
                            if filter_l.is_empty() || name.to_lowercase().starts_with(&filter_l) {
                                let kind = if matches!(
                                    *name,
                                    "const"
                                        | "let"
                                        | "var"
                                        | "if"
                                        | "for"
                                        | "while"
                                        | "return"
                                        | "await"
                                        | "async"
                                        | "typeof"
                                        | "new"
                                        | "throw"
                                        | "try"
                                ) {
                                    CompletionItemKind::KEYWORD
                                } else if matches!(*name, "require") {
                                    CompletionItemKind::FUNCTION
                                } else {
                                    CompletionItemKind::VARIABLE
                                };
                                items.push(CompletionItem {
                                    label: name.to_string(),
                                    kind: Some(kind),
                                    detail: Some(format!("script — {desc}")),
                                    insert_text: Some(name.to_string()),
                                    filter_text: Some(name.to_string()),
                                    sort_text: Some(format!("0{name}")),
                                    documentation: Some(Documentation::String(desc.to_string())),
                                    ..Default::default()
                                });
                            }
                        }
                        for (label, detail, body) in SCRIPT_SNIPPETS {
                            if filter_l.is_empty()
                                || label.to_lowercase().contains(&filter_l)
                                || detail.to_lowercase().contains(&filter_l)
                            {
                                items.push(CompletionItem {
                                    label: label.to_string(),
                                    kind: Some(CompletionItemKind::SNIPPET),
                                    detail: Some(detail.to_string()),
                                    insert_text: Some(body.to_string()),
                                    insert_text_format: Some(InsertTextFormat::SNIPPET),
                                    sort_text: Some(format!("9{label}")),
                                    ..Default::default()
                                });
                            }
                        }
                    }
                    "response" => push_props(&mut items, RESPONSE_PROPS, "1"),
                    "response.headers" => push_props(&mut items, RESPONSE_HEADERS_PROPS, "1"),
                    "request" => push_props(&mut items, REQUEST_PROPS, "1"),
                    "client" => push_props(&mut items, CLIENT_PROPS, "1"),
                    "client.global" => push_props(&mut items, CLIENT_GLOBAL_PROPS, "1"),
                    "JSON" => push_props(&mut items, JSON_PROPS, "1"),
                    "Math" => push_props(&mut items, MATH_PROPS, "1"),
                    "Object" => push_props(&mut items, OBJECT_PROPS, "1"),
                    "Array" => push_props(&mut items, ARRAY_PROPS, "1"),
                    "Date" => push_props(&mut items, DATE_PROPS, "1"),
                    "console" => {
                        for name in ["log", "error", "warn", "info"] {
                            if filter_l.is_empty() || name.starts_with(filter_l.as_str()) {
                                items.push(CompletionItem {
                                    label: name.to_string(),
                                    kind: Some(CompletionItemKind::METHOD),
                                    detail: Some("console".to_string()),
                                    insert_text: Some(format!("{name}($1)")),
                                    insert_text_format: Some(InsertTextFormat::SNIPPET),
                                    filter_text: Some(name.to_string()),
                                    ..Default::default()
                                });
                            }
                        }
                    }
                    _ => {}
                }
            }

            // require('…') module name completion inside script
            if let Some(mod_partial) = require_module_partial(before_cursor) {
                let pl = mod_partial.to_lowercase();
                for m in REQUIRE_MODULES {
                    if pl.is_empty() || m.starts_with(pl.as_str()) {
                        items.push(CompletionItem {
                            label: format!("'{m}'"),
                            kind: Some(CompletionItemKind::MODULE),
                            detail: Some("Node/httpyac require module".to_string()),
                            insert_text: Some(format!("'{m}'")),
                            filter_text: Some(m.to_string()),
                            sort_text: Some(format!("0{m}")),
                            ..Default::default()
                        });
                    }
                }
            }

            // Snippets when line starts with `>` 
            if trimmed.starts_with('>') || trimmed.is_empty() && full_line_trimmed.starts_with('>')
            {
                for (label, detail, body) in SCRIPT_SNIPPETS {
                    items.push(CompletionItem {
                        label: label.to_string(),
                        kind: Some(CompletionItemKind::SNIPPET),
                        detail: Some(detail.to_string()),
                        insert_text: Some(body.to_string()),
                        insert_text_format: Some(InsertTextFormat::SNIPPET),
                        sort_text: Some(format!("0{label}")),
                        ..Default::default()
                    });
                }
            }

            if !items.is_empty() {
                return Ok(Some(CompletionResponse::Array(items)));
            }
        }

        // Response handler starter on empty line after request
        if trimmed.is_empty() || trimmed == ">" {
            let block_start = (0..=line_idx)
                .rev()
                .find(|&i| lines[i].trim_start().starts_with("###"))
                .map(|i| i + 1)
                .unwrap_or(0);
            let has_req = (block_start..line_idx).any(|i| {
                let l = lines[i].trim();
                let mut p = l.split_whitespace();
                let m = p.next().unwrap_or("").to_ascii_uppercase();
                p.next().is_some()
                    && matches!(
                        m.as_str(),
                        "GET" | "POST" | "PUT" | "PATCH" | "DELETE" | "HEAD" | "OPTIONS"
                    )
            });
            if has_req {
                items.push(CompletionItem {
                    label: "> {% … %}".to_string(),
                    kind: Some(CompletionItemKind::SNIPPET),
                    detail: Some("httpyac / IntelliJ response handler".to_string()),
                    insert_text: Some(
                        "> {%\n  client.global.set(\"${1:name}\", response.parsedBody${2:});\n%}"
                            .to_string(),
                    ),
                    insert_text_format: Some(InsertTextFormat::SNIPPET),
                    sort_text: Some("0handler".to_string()),
                    ..Default::default()
                });
                items.push(CompletionItem {
                    label: "{{ … }} script".to_string(),
                    kind: Some(CompletionItemKind::SNIPPET),
                    detail: Some("httpyac response script block".to_string()),
                    insert_text: Some(
                        "{{\n  client.test(\"${1:ok}\", () => {\n    client.assert(response.statusCode === 200);\n  });\n}}"
                            .to_string(),
                    ),
                    insert_text_format: Some(InsertTextFormat::SNIPPET),
                    sort_text: Some("0script".to_string()),
                    ..Default::default()
                });
            }
        }

        let line_has_url =
            trimmed.contains("http://") || trimmed.contains("https://") || trimmed.contains("://");
        let line_has_colon_header = trimmed.contains(':') && !line_has_url;

        let block_start = (0..=line_idx)
            .rev()
            .find(|&i| lines[i].trim_start().starts_with("###"))
            .map(|i| i + 1)
            .unwrap_or(0);

        let has_request_line_before_cursor = (block_start..line_idx).any(|i| {
            let l = lines[i].trim();
            if l.is_empty() || l.starts_with('#') || l.starts_with("//") {
                return false;
            }
            let mut parts = l.split_whitespace();
            let first = parts.next().unwrap_or("").to_ascii_uppercase();
            let second = parts.next().unwrap_or("");
            !second.is_empty()
                && matches!(
                    first.as_str(),
                    "GET" | "POST" | "PUT" | "PATCH" | "DELETE" | "HEAD" | "OPTIONS"
                        | "CONNECT" | "TRACE"
                )
        });

        let first_meaningful_line_in_block = (block_start..=line_idx).find(|&i| {
            let l = lines[i].trim();
            !l.is_empty() && !l.starts_with('#') && !l.starts_with("//")
        });
        let is_first_meaningful_line = first_meaningful_line_in_block == Some(line_idx);

        // --- HTTP methods ---
        let is_empty_line = trimmed.is_empty();
        let is_method_prefix = if trimmed.is_empty() {
            false
        } else {
            let trimmed_upper = trimmed.to_ascii_uppercase();
            trimmed.chars().all(|c| c.is_ascii_alphabetic())
                && HTTP_METHODS
                    .iter()
                    .any(|(m, _)| m.starts_with(&trimmed_upper))
        };
        let at_method_position = is_empty_line || is_method_prefix;

        if at_method_position
            && !line_has_url
            && !line_has_colon_header
            && is_first_meaningful_line
            && !has_request_line_before_cursor
        {
            let prefix_upper = trimmed.to_uppercase();
            for (method, desc) in HTTP_METHODS {
                if prefix_upper.is_empty() || method.starts_with(&prefix_upper) {
                    items.push(CompletionItem {
                        label: method.to_string(),
                        kind: Some(CompletionItemKind::KEYWORD),
                        detail: Some(desc.to_string()),
                        text_edit: Some(CompletionTextEdit::Edit(TextEdit {
                            range: Range {
                                start: Position {
                                    line: line_num,
                                    character: line_start_char,
                                },
                                end: Position {
                                    line: line_num,
                                    character: position.character,
                                },
                            },
                            new_text: format!("{method} "),
                        })),
                        filter_text: Some(method.to_string()),
                        sort_text: Some(format!("0{method}")),
                        ..Default::default()
                    });
                }
            }
        }

        // --- Header names (after request line; "Ho" → Host, empty line → all headers) ---
        // Prefer headers over document-word completions (Zed may mix both).
        let looks_like_header_token = is_empty_line
            || (trimmed
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
                && !trimmed.contains(' '));
        let typing_header_name = !line_has_colon_header
            && !line_has_url
            && has_request_line_before_cursor
            && !trimmed.starts_with('>')
            && !trimmed.starts_with('{')
            && !trimmed.starts_with('@')
            && looks_like_header_token
            // Don't treat pure method keywords on first line as headers
            && !(is_first_meaningful_line && is_method_prefix && !has_request_line_before_cursor);

        if typing_header_name {
            let prefix = trimmed.to_lowercase();
            for (header, default_value) in HEADER_NAMES {
                let h_lower = header.to_lowercase();
                if !prefix.is_empty() && !h_lower.starts_with(&prefix) {
                    continue;
                }
                let detail = if default_value.is_empty() {
                    "HTTP header".to_string()
                } else {
                    format!("e.g. {default_value}")
                };
                // Rank common headers first so Host beats buffer words like "Hello"
                let sort = if h_lower == "host" {
                    "00Host".to_string()
                } else if matches!(
                    h_lower.as_str(),
                    "content-type"
                        | "authorization"
                        | "accept"
                        | "user-agent"
                        | "cookie"
                        | "content-length"
                ) {
                    format!("01{header}")
                } else {
                    format!("02{header}")
                };
                let preselect = h_lower == "host" && (prefix == "h" || prefix == "ho" || prefix == "hos" || prefix == "host");
                items.push(CompletionItem {
                    label: header.to_string(),
                    kind: Some(CompletionItemKind::PROPERTY),
                    detail: Some(detail),
                    // filter_text helps clients match case-insensitively
                    filter_text: Some(format!("{header} {h_lower}")),
                    sort_text: Some(sort),
                    preselect: if preselect { Some(true) } else { None },
                    text_edit: Some(CompletionTextEdit::Edit(TextEdit {
                        range: Range {
                            start: Position {
                                line: line_num,
                                character: line_start_char,
                            },
                            end: Position {
                                line: line_num,
                                character: position.character,
                            },
                        },
                        new_text: format!("{header}: "),
                    })),
                    documentation: Some(Documentation::String(
                        "HTTP request header (httpyac-lsp)".to_string(),
                    )),
                    ..Default::default()
                });
            }
        }

        // --- Header values ---
        if line_has_colon_header {
            let colon_pos = trimmed.find(':').unwrap_or(0);
            let header_name = trimmed[..colon_pos].trim().to_lowercase();
            let after_colon = trimmed[colon_pos + 1..].trim();
            let value_start_char = if let Some(raw_colon) = before_cursor.find(':') {
                let after = &before_cursor[raw_colon + 1..];
                let spaces = after.len() - after.trim_start().len();
                (raw_colon + 1 + spaces) as u32
            } else {
                position.character
            };

            for val in header_values(&header_name) {
                if after_colon.is_empty()
                    || val.to_lowercase().starts_with(&after_colon.to_lowercase())
                {
                    items.push(CompletionItem {
                        label: format!("{header_name}: {val}"),
                        kind: Some(CompletionItemKind::VALUE),
                        detail: Some(header_name.clone()),
                        text_edit: Some(CompletionTextEdit::Edit(TextEdit {
                            range: Range {
                                start: Position {
                                    line: line_num,
                                    character: value_start_char,
                                },
                                end: Position {
                                    line: line_num,
                                    character: position.character,
                                },
                            },
                            new_text: val.to_string(),
                        })),
                        sort_text: Some(format!("0{val}")),
                        ..Default::default()
                    });
                }
            }
            if header_name == "authorization" {
                for val in AUTH_SCHEMES {
                    if after_colon.is_empty()
                        || val.to_lowercase().starts_with(&after_colon.to_lowercase())
                    {
                        items.push(CompletionItem {
                            label: format!("{header_name}: {}", val.trim()),
                            kind: Some(CompletionItemKind::VALUE),
                            detail: Some("Auth scheme".to_string()),
                            text_edit: Some(CompletionTextEdit::Edit(TextEdit {
                                range: Range {
                                    start: Position {
                                        line: line_num,
                                        character: value_start_char,
                                    },
                                    end: Position {
                                        line: line_num,
                                        character: position.character,
                                    },
                                },
                                new_text: val.to_string(),
                            })),
                            ..Default::default()
                        });
                    }
                }
            }
        }

        // @file variable declaration on empty-ish line
        if trimmed.starts_with('@') && !trimmed.contains('=') {
            items.push(CompletionItem {
                label: "@variable = value".to_string(),
                kind: Some(CompletionItemKind::SNIPPET),
                detail: Some("File-level variable".to_string()),
                insert_text: Some("@${1:name} = ${2:value}".to_string()),
                insert_text_format: Some(InsertTextFormat::SNIPPET),
                sort_text: Some("0@var".to_string()),
                ..Default::default()
            });
        }

        if items.is_empty() {
            Ok(None)
        } else {
            Ok(Some(CompletionResponse::Array(items)))
        }
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }
}

#[tokio::main]
async fn main() {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::build(HttpLsp::new)
        .custom_method("experimental/runnables", HttpLsp::handle_runnables)
        .finish();
    Server::new(stdin, stdout, socket).serve(service).await;
}
