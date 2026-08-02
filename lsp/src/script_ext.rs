//! Script completions from **builtin** + user `.script/` JS modules.
//!
//! ## Load order
//!
//! 1. **Builtin** (`lsp/builtin_script/*.js`, embedded via `include_str!`)
//!    — `request`, `response`, `client`, `console`, `crypto`, …
//! 2. **User** project `.script/*.js` (walk parents from the `.http` file)
//! 3. **User wins** on the same module path / member name (override).
//!
//! ## User convention
//!
//! ```text
//! .script/
//!   crypto.js             → overrides/extends builtin crypto
//!   request.headers.js    → path extension request.headers.*
//!   helpers.js            → new module helpers
//! ```
//!
//! Any `.js` / `.mjs` / `.cjs` is heuristically parsed for exports / nested objects.
//! Completions are **editor-only**. Runtime still uses httpyac + real `require()`.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// One completion member (method / property / nested object).
#[derive(Debug, Clone)]
pub struct ScriptMember {
    pub name: String,
    pub detail: String,
    pub documentation: String,
    /// Optional snippet insert (e.g. `set(${1:name}, ${2:value})`).
    pub insert: Option<String>,
    pub is_method: bool,
    pub source: String,
    /// Declared/inferred return type (`Hmac`, `string`, `Buffer`, …) from `@returns {T}`.
    pub returns: Option<String>,
}

impl ScriptMember {
    fn with_returns(mut self, returns: Option<String>) -> Self {
        if let Some(ref t) = returns {
            if !self.detail.contains('→') {
                self.detail = format!("{} → {t}", self.detail);
            }
            if !self.documentation.contains("@returns") {
                self.documentation = format!("{}\n@returns {{{t}}}", self.documentation);
            }
        }
        self.returns = returns;
        self
    }
}

/// Nested member tree under a module or path.
#[derive(Debug, Clone, Default)]
pub struct MemberTree {
    /// Direct children by name.
    pub children: HashMap<String, MemberTree>,
    /// Leaf metadata when this node is selectable / has a call form.
    pub member: Option<ScriptMember>,
}

impl MemberTree {
    pub fn ensure_child(&mut self, name: &str) -> &mut MemberTree {
        self.children
            .entry(name.to_string())
            .or_insert_with(MemberTree::default)
    }

    /// Insert a leaf at dotted path relative to this node (`a.b.c`).
    pub fn insert_path(&mut self, path: &str, member: ScriptMember) {
        let parts: Vec<&str> = path.split('.').filter(|p| !p.is_empty()).collect();
        if parts.is_empty() {
            return;
        }
        let mut node = self;
        for (i, part) in parts.iter().enumerate() {
            let is_last = i + 1 == parts.len();
            node = node.ensure_child(part);
            if is_last {
                // Keep nested children if re-declared as both object and method.
                let children = std::mem::take(&mut node.children);
                node.member = Some(member.clone());
                node.children = children;
            }
        }
    }

    /// Members directly under `path` (empty path = this node’s children).
    pub fn members_at(&self, path: &str) -> Vec<&ScriptMember> {
        let node = self.walk(path);
        let Some(node) = node else {
            return Vec::new();
        };
        let mut out: Vec<&ScriptMember> = node
            .children
            .iter()
            .filter_map(|(name, child)| {
                if let Some(m) = &child.member {
                    Some(m)
                } else if !child.children.is_empty() {
                    // Namespace object with no leaf — synthesize a property hint
                    None
                } else {
                    let _ = name;
                    None
                }
            })
            .collect();
        // Also expose bare child names that are only namespaces (so user can type `.utils.`)
        for (name, child) in &node.children {
            if child.member.is_none() && !child.children.is_empty() {
                // create a temporary? We need owned or static — use member on the fly via
                // storing synthetic members. Better: ensure every namespace has a member.
                let _ = name;
            }
        }
        // Prefer deterministic order
        out.sort_by(|a, b| a.name.cmp(&b.name));
        out
    }

    /// Child names at path, including pure namespaces (for completion list).
    pub fn child_entries_at(&self, path: &str) -> Vec<ScriptMember> {
        let Some(node) = self.walk(path) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for (name, child) in &node.children {
            if let Some(m) = &child.member {
                out.push(m.clone());
            } else if !child.children.is_empty() {
                out.push(ScriptMember {
                    name: name.clone(),
                    detail: format!("object — {} nested members", child.children.len()),
                    documentation: format!("Namespace `{name}` from .script/"),
                    insert: None,
                    is_method: false,
                    source: child
                        .children
                        .values()
                        .find_map(|c| c.member.as_ref().map(|m| m.source.clone()))
                        .unwrap_or_else(|| ".script".into()),
                    returns: None,
                });
            }
        }
        out.sort_by(|a, b| a.name.cmp(&b.name));
        out
    }

    fn walk(&self, path: &str) -> Option<&MemberTree> {
        if path.is_empty() {
            return Some(self);
        }
        let mut node = self;
        for part in path.split('.').filter(|p| !p.is_empty()) {
            node = node.children.get(part)?;
        }
        Some(node)
    }

    /// Merge without clobbering existing leaf members (fill gaps only).
    pub fn merge_from(&mut self, other: &MemberTree) {
        for (k, v) in &other.children {
            let slot = self.ensure_child(k);
            if slot.member.is_none() {
                slot.member = v.member.clone();
            }
            slot.merge_from(v);
        }
    }

    /// Merge with **override**: `other` replaces same-named members (user over builtin).
    /// If the override omits `returns`, keep the previous return type (so user
    /// `.script/crypto.js` can add helpers without breaking `createHmac → Hmac` chains).
    pub fn merge_override(&mut self, other: &MemberTree) {
        for (k, v) in &other.children {
            let slot = self.ensure_child(k);
            if let Some(mut new_m) = v.member.clone() {
                if new_m.returns.is_none() {
                    if let Some(old) = &slot.member {
                        if let Some(r) = &old.returns {
                            new_m.returns = Some(r.clone());
                            if !new_m.detail.contains('→') {
                                new_m.detail = format!("{} → {r}", new_m.detail);
                            }
                        }
                    }
                }
                // Well-known fallbacks when neither side declared returns
                if new_m.returns.is_none() {
                    if let Some(r) = default_returns_for_method(&new_m.name) {
                        new_m.returns = Some(r.to_string());
                        if !new_m.detail.contains('→') {
                            new_m.detail = format!("{} → {r}", new_m.detail);
                        }
                    }
                }
                slot.member = Some(new_m);
            }
            slot.merge_override(v);
        }
    }
}

/// When JSDoc is missing, still chain common Node/httpyac APIs.
fn default_returns_for_method(name: &str) -> Option<&'static str> {
    match name {
        "createHmac" => Some("Hmac"),
        "createHash" => Some("Hash"),
        "createSign" => Some("Sign"),
        "createVerify" => Some("Verify"),
        "randomBytes" | "pbkdf2Sync" | "scryptSync" | "publicEncrypt" | "privateDecrypt" => {
            Some("Buffer")
        }
        // Do NOT map bare `update`/`copy` → Hmac: that poisons Hash/Sign chains.
        // Those methods declare @returns on type.Hmac.js / type.Hash.js.
        "digest" => Some("string"),
        _ => None,
    }
}

/// Full catalog: builtin scripts + optional user `.script/`.
#[derive(Debug, Clone, Default)]
pub struct ScriptCatalog {
    pub dir: PathBuf,
    /// File stem → member tree (`crypto.js` → key `crypto`).
    pub modules: HashMap<String, MemberTree>,
    /// Dotted path extensions (`request.headers` → tree of set/get/…).
    pub path_extensions: HashMap<String, MemberTree>,
    /// Named return types (`Hmac`, `Hash`, …) from `@httpyac-type` / `type.Name.js`.
    pub types: HashMap<String, MemberTree>,
}

impl ScriptCatalog {
    /// Merge another catalog on top (user override).
    pub fn merge_user_override(&mut self, user: &ScriptCatalog) {
        for (name, tree) in &user.modules {
            let slot = self
                .modules
                .entry(name.clone())
                .or_insert_with(MemberTree::default);
            slot.merge_override(tree);
        }
        for (name, tree) in &user.path_extensions {
            let slot = self
                .path_extensions
                .entry(name.clone())
                .or_insert_with(MemberTree::default);
            slot.merge_override(tree);
        }
        for (name, tree) in &user.types {
            let slot = self
                .types
                .entry(name.clone())
                .or_insert_with(MemberTree::default);
            slot.merge_override(tree);
        }
        if user.dir.as_os_str().len() > 0 {
            self.dir = user.dir.clone();
        }
    }

    /// Members of a named type (`Hmac`, `Buffer`, `$ret_foo`, …). Optional `rest` sub-path.
    pub fn members_for_type(&self, type_name: &str, rest: &str) -> Vec<ScriptMember> {
        let t = unwrap_promise_type(type_name.trim());
        // Raw object shape string (not yet in catalog.types)
        if t.starts_with('{') {
            if let Some(fields) = parse_object_shape_fields(t) {
                return fields
                    .into_iter()
                    .map(|f| ScriptMember {
                        name: f.name.clone(),
                        detail: if f.is_method {
                            format!("method() → {}", f.type_str)
                        } else {
                            f.type_str.clone()
                        },
                        documentation: format!(
                            "{} `{}`",
                            if f.is_method { "Method" } else { "Property" },
                            f.name
                        ),
                        insert: if f.is_method {
                            Some(format!("{}($1)", f.name))
                        } else {
                            None
                        },
                        is_method: f.is_method,
                        source: "inferred".into(),
                        returns: if f.is_method && f.type_str != "any" {
                            Some(f.type_str)
                        } else {
                            None
                        },
                    })
                    .collect();
            }
        }
        for part in t.split('|') {
            let p = part.trim();
            if p.is_empty() || is_primitive_type(p) {
                continue;
            }
            let p = p.trim_end_matches("[]");
            if let Some(tree) = self.types.get(p) {
                return tree.child_entries_at(rest);
            }
        }
        Vec::new()
    }

    /// Find `@returns` / inferred return of an export `name` on any loaded module.
    pub fn find_returns_for_export(&self, name: &str) -> Option<String> {
        if let Some(r) = self.returns_of_path(&format!("$script.{name}")) {
            return Some(r);
        }
        for (mod_name, tree) in &self.modules {
            if mod_name == "$script" {
                continue;
            }
            if let Some(r) = member_returns_at(tree, name) {
                return Some(r);
            }
        }
        None
    }

    /// Return type of module path `crypto.createHmac` (no call parens).
    pub fn returns_of_path(&self, path: &str) -> Option<String> {
        let path = path.trim().trim_end_matches('(').trim();
        if path.is_empty() {
            return None;
        }
        let mut parts = path.splitn(2, '.');
        let head = parts.next()?;
        let rest = parts.next().unwrap_or("");
        if rest.is_empty() {
            // bare method name fallback
            return default_returns_for_method(head).map(|s| s.to_string());
        }
        let last = rest.rsplit('.').next().unwrap_or(rest);

        if let Some(tree) = self.modules.get(head) {
            if let Some(r) = member_returns_at(tree, rest) {
                return Some(r);
            }
        }
        for (ext, tree) in &self.path_extensions {
            if let Some(sub) = path.strip_prefix(ext).and_then(|r| r.strip_prefix('.')) {
                if let Some(r) = member_returns_at(tree, sub) {
                    return Some(r);
                }
            }
        }
        if let Some(tree) = self.types.get(head) {
            if let Some(r) = member_returns_at(tree, rest) {
                return Some(r);
            }
        }
        // Fallback by method name (createHmac / update / …)
        default_returns_for_method(last).map(|s| s.to_string())
    }

    /// Resolve members for a completion path (after require-alias rewrite).
    ///
    /// - `crypto` / `crypto.createHmac` → module tree
    /// - `request.headers` → path extension **merged on top of** `request` module’s `headers`
    /// - bare module stem with empty filter handled by caller
    pub fn members_for_path(&self, path: &str) -> Vec<ScriptMember> {
        if path.is_empty() {
            return Vec::new();
        }

        // Build effective tree: module walk, then overlay matching path_extensions.
        let mut base: Option<Vec<ScriptMember>> = None;

        // Module: first segment is module name (request.headers → request + headers)
        let mut parts = path.splitn(2, '.');
        let head = parts.next().unwrap_or("");
        let rest = parts.next().unwrap_or("");
        if let Some(tree) = self.modules.get(head) {
            base = Some(tree.child_entries_at(rest));
        }

        // Path extension exact / prefix (request.headers.js → request.headers)
        let mut ext_members: Vec<ScriptMember> = Vec::new();
        if let Some(tree) = self.path_extensions.get(path) {
            ext_members = tree.child_entries_at("");
        } else {
            for (ext_path, tree) in &self.path_extensions {
                if path == *ext_path || path.starts_with(&format!("{ext_path}.")) {
                    let rest = path
                        .strip_prefix(ext_path)
                        .unwrap_or("")
                        .strip_prefix('.')
                        .unwrap_or("");
                    ext_members = tree.child_entries_at(rest);
                    break;
                }
            }
        }

        // Merge: start with module members, extension/user overlay by name
        let mut by_name: HashMap<String, ScriptMember> = HashMap::new();
        if let Some(b) = base {
            for m in b {
                by_name.insert(m.name.clone(), m);
            }
        }
        for m in ext_members {
            by_name.insert(m.name.clone(), m); // extension wins
        }
        let mut out: Vec<_> = by_name.into_values().collect();
        out.sort_by(|a, b| a.name.cmp(&b.name));
        out
    }

    /// Top-level module names for bare identifier completion.
    pub fn module_roots(&self) -> Vec<(String, String)> {
        let mut v: Vec<_> = self
            .modules
            .iter()
            .map(|(k, _)| {
                let detail = if self.dir.as_os_str().is_empty() {
                    format!("builtin:{k}.js")
                } else {
                    format!("script:{k} (builtin + user .script)")
                };
                (k.clone(), detail)
            })
            .collect();
        v.sort_by(|a, b| a.0.cmp(&b.0));
        v
    }
}

/// Embedded builtin scripts (same format as user `.script/*.js`).
const BUILTIN_SCRIPTS: &[(&str, &str)] = &[
    ("request.js", include_str!("../builtin_script/request.js")),
    ("response.js", include_str!("../builtin_script/response.js")),
    ("client.js", include_str!("../builtin_script/client.js")),
    ("console.js", include_str!("../builtin_script/console.js")),
    ("crypto.js", include_str!("../builtin_script/crypto.js")),
    ("fs.js", include_str!("../builtin_script/fs.js")),
    ("path.js", include_str!("../builtin_script/path.js")),
    ("Buffer.js", include_str!("../builtin_script/Buffer.js")),
    ("JSON.js", include_str!("../builtin_script/JSON.js")),
    ("Math.js", include_str!("../builtin_script/Math.js")),
    ("Object.js", include_str!("../builtin_script/Object.js")),
    ("Array.js", include_str!("../builtin_script/Array.js")),
    ("Date.js", include_str!("../builtin_script/Date.js")),
    ("exports.js", include_str!("../builtin_script/exports.js")),
    // Return-type shapes for chaining (const h = crypto.createHmac(); h.)
    ("type.Hmac.js", include_str!("../builtin_script/type.Hmac.js")),
    ("type.Hash.js", include_str!("../builtin_script/type.Hash.js")),
    ("type.Sign.js", include_str!("../builtin_script/type.Sign.js")),
    ("type.Verify.js", include_str!("../builtin_script/type.Verify.js")),
    ("type.Buffer.js", include_str!("../builtin_script/type.Buffer.js")),
];

fn is_primitive_type(t: &str) -> bool {
    let t = t.trim();
    matches!(
        t,
        "string"
            | "number"
            | "boolean"
            | "bool"
            | "void"
            | "null"
            | "undefined"
            | "any"
            | "object"
            | "Object"
            | "symbol"
            | "bigint"
            | "unknown"
            | "never"
    )
}

/// `Promise<T>` / `Promise<T>` → `T` (httpyac scripts often ignore await in tips).
fn unwrap_promise_type(t: &str) -> &str {
    let t = t.trim();
    let lower = t.to_ascii_lowercase();
    if let Some(rest) = lower.strip_prefix("promise<") {
        if t.ends_with('>') {
            let inner_len = rest.len().saturating_sub(1); // strip matching on lower only
            // Use original case inner
            let start = t.find('<').map(|i| i + 1).unwrap_or(0);
            let end = t.rfind('>').unwrap_or(t.len());
            if start < end {
                return t[start..end].trim();
            }
            let _ = inner_len;
        }
    }
    t
}

/// Parse local functions / object shapes from the open `{{ }}` script window into catalog.
pub fn ingest_script_locals(catalog: &mut ScriptCatalog, script_text: &str) {
    let mut tmp = ScriptCatalog::default();
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        ingest_js_source(&mut tmp, "$script.js", script_text, true);
    }));
    if let Some(tree) = tmp.modules.remove("$script") {
        catalog
            .modules
            .entry("$script".into())
            .or_insert_with(MemberTree::default)
            .merge_override(&tree);
    }
    // Also copy any other stems (unlikely from a script snippet)
    for (k, tree) in tmp.modules {
        catalog
            .modules
            .entry(k)
            .or_insert_with(MemberTree::default)
            .merge_override(&tree);
    }
    for (k, tree) in tmp.types {
        catalog
            .types
            .entry(k)
            .or_insert_with(MemberTree::default)
            .merge_override(&tree);
    }
    for (k, tree) in tmp.path_extensions {
        catalog
            .path_extensions
            .entry(k)
            .or_insert_with(MemberTree::default)
            .merge_override(&tree);
    }
}

/// Infer `@returns`-like object types from `return { … }` inside function bodies.
/// Covers: `function name`, `const name = …`, method shorthand `name(){…}`,
/// `exports.name = …` / `module.exports.name = …` (single **or** mixed shapes).
fn infer_returns_from_function_bodies(original: &str, stripped: &str) -> HashMap<String, String> {
    let mut out = HashMap::new();
    // function name(...) { … return { … } }
    let mut search = stripped;
    while let Some(idx) = search.find("function") {
        let after = search[idx + "function".len()..].trim_start();
        if after.starts_with('(') {
            // anonymous — skip (handled via exports.x = function() …)
            search = &search[idx + 8..];
            continue;
        }
        let Some(name) = take_ident(after) else {
            search = &search[idx + 8..];
            continue;
        };
        let after_name = after[name.len()..].trim_start();
        if !after_name.starts_with('(') {
            search = &search[idx + 8..];
            continue;
        }
        let Some(params) = extract_balanced(&after_name[1..], '(', ')') else {
            search = &search[idx + 8..];
            continue;
        };
        let after_params = after_name[1 + params.len() + 1..].trim_start();
        if !after_params.starts_with('{') {
            search = &search[idx + 8..];
            continue;
        }
        let Some(body) = extract_balanced(&after_params[1..], '{', '}') else {
            search = &search[idx + 8..];
            continue;
        };
        if let Some(obj_ty) = first_return_object_type(body) {
            out.insert(name, obj_ty);
        }
        search = &search[idx + 8..];
    }

    // const/let/var name = (…) => ({ … })  or  = function …
    for kw in ["const ", "let ", "var "] {
        let mut search = stripped;
        while let Some(idx) = search.find(kw) {
            let after = &search[idx + kw.len()..];
            if let Some(name) = take_ident(after) {
                let rest = after[name.len()..].trim_start();
                if let Some(rest) = rest.strip_prefix('=') {
                    let rest = rest.trim_start();
                    if let Some(obj_ty) = arrow_or_fn_expr_return_object(rest) {
                        out.entry(name).or_insert(obj_ty);
                    }
                }
            }
            search = &search[idx + kw.len()..];
        }
    }

    // exports.foo = function… / module.exports.bar = () => ({…})
    for (name, obj_ty) in infer_returns_from_exports_assigns(stripped) {
        out.entry(name).or_insert(obj_ty);
    }

    // Method shorthand: name() { return { … } }  (module.exports = { name(){…} })
    for (name, obj_ty) in infer_returns_from_method_shorthands(stripped) {
        out.entry(name).or_insert(obj_ty);
    }

    let _ = original;
    out
}

/// `exports.foo = <fn>` / `module.exports.foo = <fn>` → return object shape.
fn infer_returns_from_exports_assigns(stripped: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for marker in ["module.exports.", "exports."] {
        let mut search = stripped;
        while let Some(idx) = search.find(marker) {
            let after = &search[idx + marker.len()..];
            if let Some(name) = take_ident(after) {
                let rest = after[name.len()..].trim_start();
                if let Some(rest) = rest.strip_prefix('=') {
                    let rest = rest.trim_start();
                    if let Some(obj_ty) = arrow_or_fn_expr_return_object(rest) {
                        out.push((name, obj_ty));
                    }
                }
            }
            search = &search[idx + marker.len()..];
        }
    }
    out
}

/// Heuristic: `name(…) { … return {…} }` method/export shorthand (not `function name`).
/// Skips control-flow keywords (`if`/`for`/…).
fn infer_returns_from_method_shorthands(stripped: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let bytes = stripped.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        // Soft boundary: avoid matching inside `obj.method` or mid-ident.
        if i > 0 {
            let prev = bytes[i - 1];
            if prev.is_ascii_alphanumeric() || prev == b'_' || prev == b'$' || prev == b'.' {
                i += 1;
                continue;
            }
        }
        // Fast-path: only try when this looks like an ident start
        if !(bytes[i].is_ascii_alphabetic() || bytes[i] == b'_' || bytes[i] == b'$') {
            i += 1;
            continue;
        }
        let rest = &stripped[i..];
        let Some(name) = take_ident(rest) else {
            i += 1;
            continue;
        };
        if is_control_flow_or_keyword(&name) {
            i += name.len().max(1);
            continue;
        }
        let after_name = rest[name.len()..].trim_start();
        if !after_name.starts_with('(') {
            i += 1;
            continue;
        }
        // Reject `function name` (already handled)
        let before = stripped[..i].trim_end();
        if before.ends_with("function") {
            i += 1;
            continue;
        }
        let Some(params) = extract_balanced(&after_name[1..], '(', ')') else {
            i += 1;
            continue;
        };
        let after_params = after_name[1 + params.len() + 1..].trim_start();
        if after_params.starts_with("=>") || !after_params.starts_with('{') {
            i += 1;
            continue;
        }
        let Some(body) = extract_balanced(&after_params[1..], '{', '}') else {
            i += 1;
            continue;
        };
        if let Some(obj_ty) = first_return_object_type(body) {
            out.push((name.clone(), obj_ty));
        }
        // Advance past this method to avoid re-scanning inside its body
        let consumed = name.len()
            + (rest[name.len()..].len() - after_name.len())
            + 1
            + params.len()
            + 1
            + (after_name[1 + params.len() + 1..].len() - after_params.len())
            + 1
            + body.len()
            + 1;
        i += consumed.max(1);
    }
    out
}

fn is_control_flow_or_keyword(name: &str) -> bool {
    matches!(
        name,
        "if" | "else"
            | "for"
            | "while"
            | "do"
            | "switch"
            | "case"
            | "try"
            | "catch"
            | "finally"
            | "with"
            | "function"
            | "class"
            | "return"
            | "throw"
            | "new"
            | "typeof"
            | "instanceof"
            | "void"
            | "delete"
            | "await"
            | "yield"
            | "import"
            | "export"
            | "from"
            | "of"
            | "in"
            | "as"
            | "get"
            | "set"
            | "static"
            | "async"
            | "let"
            | "const"
            | "var"
            | "module"
            | "exports"
            | "require"
            | "true"
            | "false"
            | "null"
            | "undefined"
            | "this"
            | "super"
    ) || is_reserved(name)
}

fn first_return_object_type(body: &str) -> Option<String> {
    let mut search = body;
    while let Some(idx) = search.find("return") {
        let after = search[idx + "return".len()..].trim_start();
        if after.starts_with('{') {
            let inner = extract_balanced(&after[1..], '{', '}')?;
            // Preserve mixed data props + methods from the literal
            if let Some(ty) = object_literal_to_type_string(inner) {
                return Some(ty);
            }
        }
        search = &search[idx + 6..];
    }
    None
}

fn arrow_or_fn_expr_return_object(rest: &str) -> Option<String> {
    let rest = rest.trim_start();
    // async?
    let rest = rest.strip_prefix("async").unwrap_or(rest).trim_start();
    if rest.starts_with("function") {
        let after = rest["function".len()..].trim_start();
        let after = if after.starts_with('(') {
            after
        } else {
            // function name(...)
            let n = take_ident(after)?;
            after[n.len()..].trim_start()
        };
        if !after.starts_with('(') {
            return None;
        }
        let params = extract_balanced(&after[1..], '(', ')')?;
        let after_params = after[1 + params.len() + 1..].trim_start();
        if after_params.starts_with('{') {
            let body = extract_balanced(&after_params[1..], '{', '}')?;
            return first_return_object_type(body);
        }
        return None;
    }
    // (args) =>
    if rest.starts_with('(') {
        let params = extract_balanced(&rest[1..], '(', ')')?;
        let after = rest[1 + params.len() + 1..].trim_start();
        let after = after.strip_prefix("=>")?.trim_start();
        if after.starts_with('(') && after[1..].trim_start().starts_with('{') {
            // => ({ ... })
            let after_paren = after[1..].trim_start();
            let inner = extract_balanced(&after_paren[1..], '{', '}')?;
            if let Some(ty) = object_literal_to_type_string(inner) {
                return Some(ty);
            }
        }
        if after.starts_with('{') {
            // => { return { } }  or  => { prop }  ambiguous
            let body = extract_balanced(&after[1..], '{', '}')?;
            if let Some(t) = first_return_object_type(body) {
                return Some(t);
            }
        }
    }
    None
}

/// Keys of a JS object literal `{ a, b: 1, c() {} }`.
fn object_literal_keys(inner: &str) -> Option<Vec<String>> {
    let obj = parse_object_lit(inner);
    if obj.props.is_empty() {
        return None;
    }
    Some(obj.props.iter().map(|(k, _)| k.clone()).collect())
}

fn member_returns_at(tree: &MemberTree, path: &str) -> Option<String> {
    let mut node = tree;
    let parts: Vec<&str> = path.split('.').filter(|p| !p.is_empty()).collect();
    if parts.is_empty() {
        return None;
    }
    for (i, part) in parts.iter().enumerate() {
        node = node.children.get(*part)?;
        if i + 1 == parts.len() {
            return node.member.as_ref().and_then(|m| m.returns.clone());
        }
    }
    None
}

/// Parse one JS source into the catalog (module stem or dotted path extension).
fn ingest_js_source(catalog: &mut ScriptCatalog, file_name: &str, content: &str, override_mode: bool) {
    let parsed = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        parse_js_exports(content, file_name)
    })) {
        Ok(p) => p,
        Err(_) => return, // never let a bad file kill the LSP
    };

    let stem = file_name
        .trim_end_matches(".js")
        .trim_end_matches(".mjs")
        .trim_end_matches(".cjs")
        .to_string();

    let apply = |slot: &mut MemberTree, tree: &MemberTree| {
        if override_mode {
            slot.merge_override(tree);
        } else {
            slot.merge_from(tree);
        }
    };

    // @httpyac-type Name  or  file type.Name.js → catalog.types
    let mut type_names = parsed.type_names;
    if type_names.is_empty() {
        if let Some(rest) = stem.strip_prefix("type.") {
            if !rest.is_empty() && !rest.contains('.') {
                type_names.push(rest.to_string());
            }
        }
    }
    for tname in &type_names {
        let slot = catalog
            .types
            .entry(tname.clone())
            .or_insert_with(MemberTree::default);
        apply(slot, &parsed.tree);
    }

    // Inline object @returns → catalog.types (`$ret_signRequest`, …)
    for (tname, tree) in &parsed.inline_types {
        let slot = catalog
            .types
            .entry(tname.clone())
            .or_insert_with(MemberTree::default);
        apply(slot, tree);
    }

    // type-only files (type.Hmac.js) should not also become modules/path_ext
    let type_only = stem.starts_with("type.") || (!type_names.is_empty() && stem.starts_with('_'));
    if !type_only {
        if stem.contains('.') {
            let slot = catalog
                .path_extensions
                .entry(stem)
                .or_insert_with(MemberTree::default);
            apply(slot, &parsed.tree);
        } else {
            let slot = catalog
                .modules
                .entry(stem)
                .or_insert_with(MemberTree::default);
            apply(slot, &parsed.tree);
        }
    }

    for (ext_path, tree) in parsed.path_attachments {
        let slot = catalog
            .path_extensions
            .entry(ext_path)
            .or_insert_with(MemberTree::default);
        apply(slot, &tree);
    }
}

/// Builtin catalog only (always available).
pub fn load_builtin_catalog() -> ScriptCatalog {
    let mut catalog = ScriptCatalog {
        dir: PathBuf::from("builtin_script"),
        modules: HashMap::new(),
        path_extensions: HashMap::new(),
        types: HashMap::new(),
    };
    for (name, src) in BUILTIN_SCRIPTS {
        ingest_js_source(&mut catalog, name, src, false);
    }
    catalog
}

/// Builtin first, then user `.script/` overriding same keys.
pub fn build_catalog(user_script_dir: Option<&Path>) -> ScriptCatalog {
    let mut catalog = load_builtin_catalog();
    if let Some(dir) = user_script_dir {
        let user = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            load_script_catalog(dir)
        })) {
            Ok(u) => u,
            Err(_) => ScriptCatalog::default(),
        };
        catalog.merge_user_override(&user);
        catalog.dir = dir.to_path_buf();
    }
    catalog
}

/// `const|let|var name = require('mod')` → (binding, module_id).
/// `module_id` is bare (`crypto`) or file stem (`auth-sign` from `./scripts/auth-sign.js`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequireBinding {
    pub name: String,
    pub module: String,
    /// Original require string when path-like (`./scripts/foo.js`); used to load the file.
    pub raw_path: Option<String>,
    /// `true` for `const m = require(...)` (m. → module exports).
    /// `false` for `const { signRequest } = require(...)` (bare name only, not full module).
    pub maps_to_module: bool,
}

/// Parse require bindings from script text (current buffer / block).
///
/// Supports:
/// - `const c = require('crypto')`
/// - `const m = require('./scripts/auth-sign.js')`
/// - `const { signRequest, hashHex } = require('./scripts/auth-sign.js')`
pub fn parse_require_bindings(text: &str) -> Vec<RequireBinding> {
    let mut out = Vec::new();
    let bytes = text.as_bytes();
    let lower = text.to_ascii_lowercase();
    let mut search_from = 0;
    while let Some(rel) = lower[search_from..].find("require(") {
        let idx = search_from + rel;
        let before = &text[..idx];
        let after = &text[idx + "require(".len()..];
        let after = after.trim_start();
        let Some(raw_mod) = parse_require_string_arg(after) else {
            search_from = idx + 7;
            continue;
        };
        let module_id = normalize_module_id(&raw_mod);
        let raw_path = if is_relative_require_path(&raw_mod) {
            Some(raw_mod.clone())
        } else {
            None
        };
        let (names, whole_module) = parse_bindings_before_require(before);
        for name in names {
            out.push(RequireBinding {
                name,
                module: module_id.clone(),
                raw_path: raw_path.clone(),
                maps_to_module: whole_module,
            });
        }
        search_from = idx + 7;
        if search_from >= bytes.len() {
            break;
        }
    }
    out
}

fn is_relative_require_path(raw: &str) -> bool {
    let s = raw.trim();
    s.starts_with("./")
        || s.starts_with("../")
        || s.starts_with(".\\")
        || s.starts_with("..\\")
        || s.ends_with(".js")
        || s.ends_with(".mjs")
        || s.ends_with(".cjs")
        || s.contains('/')
        || s.contains('\\')
}

/// Single `ident` or destructured `{ a, b: c }` names before `= require(`.
/// Returns (names, maps_to_module).
fn parse_bindings_before_require(before: &str) -> (Vec<String>, bool) {
    let chunk = before
        .rsplit([';', '\n'])
        .next()
        .unwrap_or(before)
        .trim();
    // Careful: rsplit on '{' breaks `const { a } = require` — use last `=` statement
    let chunk = chunk
        .trim_start_matches("const")
        .trim_start_matches("let")
        .trim_start_matches("var")
        .trim_start();
    let Some(eq) = chunk.rfind('=') else {
        return (Vec::new(), false);
    };
    let left = chunk[..eq].trim();
    if left.starts_with('{') {
        return (parse_destructure_names(left), false);
    }
    if left.starts_with('[') {
        return (Vec::new(), false);
    }
    let name: String = left
        .chars()
        .rev()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '$')
        .collect::<String>()
        .chars()
        .rev()
        .collect();
    if name.is_empty() || name.parse::<u32>().is_ok() || is_reserved(&name) {
        (Vec::new(), false)
    } else {
        (vec![name], true)
    }
}

/// `{ signRequest, hashHex as h }` → `signRequest`, `h` (alias) / `hashHex`
fn parse_destructure_names(left: &str) -> Vec<String> {
    let inner = left
        .trim()
        .strip_prefix('{')
        .and_then(|s| s.strip_suffix('}'))
        .unwrap_or(left);
    let mut names = Vec::new();
    for part in inner.split(',') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        // `foo: bar` or `foo as bar` — binding is the local name (right)
        let local = if let Some((_, rhs)) = part.split_once(':') {
            rhs.trim()
        } else if let Some((lhs, rhs)) = part.split_once(" as ") {
            let _ = lhs;
            rhs.trim()
        } else {
            part
        };
        let name: String = local
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '$')
            .collect();
        if !name.is_empty() && !is_reserved(&name) {
            names.push(name);
        }
    }
    names
}

fn parse_require_string_arg(after: &str) -> Option<String> {
    let after = after.trim_start();
    let (q, rest) = if let Some(r) = after.strip_prefix('\'') {
        ('\'', r)
    } else if let Some(r) = after.strip_prefix('"') {
        ('"', r)
    } else if let Some(r) = after.strip_prefix('`') {
        ('`', r)
    } else {
        return None;
    };
    let end = rest.find(q)?;
    Some(rest[..end].to_string())
}

fn normalize_module_id(raw: &str) -> String {
    let s = raw.trim();
    let s = s.strip_prefix("node:").unwrap_or(s);
    // ./foo/bar.js → bar ; .script/crypto → crypto ; crypto → crypto
    let path = s.replace('\\', "/");
    let base = path
        .rsplit('/')
        .next()
        .unwrap_or(&path)
        .trim_end_matches(".js")
        .trim_end_matches(".mjs")
        .trim_end_matches(".cjs");
    base.to_string()
}

/// Resolve `./scripts/foo.js` against the HTTP file's directory (and simple fallbacks).
pub fn resolve_require_js_path(http_file_dir: &Path, raw: &str) -> Option<PathBuf> {
    let raw = raw.trim();
    if raw.is_empty() {
        return None;
    }
    let mut candidates: Vec<PathBuf> = Vec::new();
    if raw.starts_with('/') {
        candidates.push(PathBuf::from(raw));
    } else {
        candidates.push(http_file_dir.join(raw));
    }
    // Allow missing extension
    if !raw.ends_with(".js") && !raw.ends_with(".mjs") && !raw.ends_with(".cjs") {
        candidates.push(http_file_dir.join(format!("{raw}.js")));
        candidates.push(http_file_dir.join(format!("{raw}.cjs")));
        candidates.push(http_file_dir.join(format!("{raw}.mjs")));
    }
    for c in &candidates {
        if c.is_file() {
            return Some(c.clone());
        }
    }
    // Normalize `./` path
    if let Ok(canon) = http_file_dir.join(raw).canonicalize() {
        if canon.is_file() {
            return Some(canon);
        }
    }
    None
}

const MAX_REQUIRE_FILE_BYTES: u64 = 512 * 1024;

/// Load every relative `require('./…js')` into `catalog.modules` under the file stem.
/// Returns require bindings (including bare `crypto`). Safe: size cap + per-file unwind.
pub fn enrich_catalog_from_requires(
    catalog: &mut ScriptCatalog,
    http_file_dir: &Path,
    script_text: &str,
) -> Vec<RequireBinding> {
    let bindings = parse_require_bindings(script_text);
    let mut loaded: HashMap<String, bool> = HashMap::new();

    for b in &bindings {
        let Some(raw) = &b.raw_path else {
            continue;
        };
        if loaded.contains_key(&b.module) {
            continue;
        }
        let Some(path) = resolve_require_js_path(http_file_dir, raw) else {
            loaded.insert(b.module.clone(), false);
            continue;
        };
        let meta_len = path.metadata().map(|m| m.len()).unwrap_or(0);
        if meta_len == 0 || meta_len > MAX_REQUIRE_FILE_BYTES {
            loaded.insert(b.module.clone(), false);
            continue;
        }
        let content = match fs::read_to_string(&path) {
            Ok(c) => c,
            Err(_) => {
                loaded.insert(b.module.clone(), false);
                continue;
            }
        };
        let file_name = path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("required.js")
            .to_string();
        let mut tmp = ScriptCatalog::default();
        let ok = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            ingest_js_source(&mut tmp, &file_name, &content, true);
        }))
        .is_ok();
        if !ok {
            loaded.insert(b.module.clone(), false);
            continue;
        }
        // Prefer tree keyed by file stem from ingest
        let stem = normalize_module_id(&file_name);
        let tree = tmp
            .modules
            .get(&stem)
            .or_else(|| tmp.modules.values().next());
        if let Some(tree) = tree {
            catalog
                .modules
                .entry(b.module.clone())
                .or_insert_with(MemberTree::default)
                .merge_override(tree);
            if stem != b.module {
                catalog
                    .modules
                    .entry(stem)
                    .or_insert_with(MemberTree::default)
                    .merge_from(tree);
            }
        }
        for (k, t) in tmp.types {
            catalog
                .types
                .entry(k)
                .or_insert_with(MemberTree::default)
                .merge_override(&t);
        }
        loaded.insert(b.module.clone(), true);
    }

    bindings
}

/// Rewrite completion path using require bindings.
/// `c.createHmac` + binding c→crypto → `crypto.createHmac`
/// Only bindings with `maps_to_module` (whole-module assign) are rewritten.
pub fn resolve_path_with_bindings(path: &str, bindings: &[RequireBinding]) -> String {
    if path.is_empty() {
        return path.to_string();
    }
    let mut parts = path.splitn(2, '.');
    let head = parts.next().unwrap_or("");
    let rest = parts.next();
    for b in bindings {
        if b.maps_to_module && b.name == head {
            return match rest {
                Some(r) if !r.is_empty() => format!("{}.{}", b.module, r),
                _ => b.module.clone(),
            };
        }
    }
    path.to_string()
}

/// Find nearest `.script` directory walking parents from `start_dir`.
pub fn find_script_dir(start_dir: &Path) -> Option<PathBuf> {
    let mut dir = start_dir.to_path_buf();
    loop {
        let candidate = dir.join(".script");
        if candidate.is_dir() {
            return Some(candidate);
        }
        if !dir.pop() {
            return None;
        }
    }
}

/// Max user `.script` files to parse (avoid hanging on huge trees copied into `.script/`).
const MAX_USER_SCRIPT_FILES: usize = 64;
/// Skip individual files larger than this (bytes).
const MAX_USER_SCRIPT_FILE_BYTES: u64 = 512 * 1024;

/// Load and parse all JS files under a user `.script/` directory (no builtins).
pub fn load_script_catalog(script_dir: &Path) -> ScriptCatalog {
    let mut catalog = ScriptCatalog {
        dir: script_dir.to_path_buf(),
        modules: HashMap::new(),
        path_extensions: HashMap::new(),
        types: HashMap::new(),
    };
    let entries = match fs::read_dir(script_dir) {
        Ok(e) => e,
        Err(_) => return catalog,
    };
    let mut files: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.is_file()
                && p.extension()
                    .and_then(|e| e.to_str())
                    .map(|e| matches!(e, "js" | "mjs" | "cjs"))
                    .unwrap_or(false)
        })
        .collect();
    files.sort();
    files.truncate(MAX_USER_SCRIPT_FILES);

    for path in files {
        let meta_len = path.metadata().map(|m| m.len()).unwrap_or(0);
        if meta_len > MAX_USER_SCRIPT_FILE_BYTES {
            continue;
        }
        let content = match fs::read_to_string(&path) {
            Ok(c) => c,
            Err(_) => continue,
        };
        let file_name = path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown.js")
            .to_string();
        // User files always override when later merged onto builtin
        ingest_js_source(&mut catalog, &file_name, &content, true);
    }

    catalog
}

/// Cached catalog: builtin + optional user `.script/` (mtime-invalidated).
#[derive(Debug, Default)]
pub struct ScriptCatalogCache {
    path: Option<PathBuf>,
    fingerprint: u64,
    catalog: ScriptCatalog,
}

impl ScriptCatalogCache {
    /// Always returns a catalog (at least builtins). Never `None`.
    pub fn get_or_load(&mut self, http_file_dir: &Path) -> &ScriptCatalog {
        let script_dir = find_script_dir(http_file_dir);
        let fp = script_dir
            .as_ref()
            .map(|d| dir_fingerprint(d))
            .unwrap_or(0);
        // Sentinel when no user dir
        let path_key = script_dir
            .clone()
            .unwrap_or_else(|| PathBuf::from("__builtin__"));
        if self.path.as_ref() == Some(&path_key) && self.fingerprint == fp {
            return &self.catalog;
        }
        self.catalog = build_catalog(script_dir.as_deref());
        self.path = Some(path_key);
        self.fingerprint = fp;
        &self.catalog
    }
}

fn dir_fingerprint(dir: &Path) -> u64 {
    let mut fp: u64 = 0;
    let Ok(rd) = fs::read_dir(dir) else {
        return 0;
    };
    for e in rd.flatten() {
        let meta = e.metadata().ok();
        let modified = meta
            .as_ref()
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let len = meta.as_ref().map(|m| m.len()).unwrap_or(0);
        fp = fp
            .wrapping_mul(31)
            .wrapping_add(modified)
            .wrapping_mul(17)
            .wrapping_add(len);
        if let Some(name) = e.file_name().to_str() {
            for b in name.bytes() {
                fp = fp.wrapping_mul(31).wrapping_add(b as u64);
            }
        }
    }
    fp
}

#[derive(Debug, Default)]
struct ParsedFile {
    tree: MemberTree,
    /// From `@httpyac-path foo.bar` blocks in the file.
    path_attachments: HashMap<String, MemberTree>,
    /// From `@httpyac-type Hmac` (return-type shapes).
    type_names: Vec<String>,
    /// Inline object return types: `$ret_signRequest` → { authDate, authentication }.
    inline_types: HashMap<String, MemberTree>,
}

/// Heuristic JS export / function parser (no full JS engine).
fn parse_js_exports(source: &str, source_name: &str) -> ParsedFile {
    let mut result = ParsedFile::default();
    let stripped = strip_js_comments(source);

    // Collect tags from original (comments kept)
    let path_tags = extract_httpyac_paths(source);
    result.type_names = extract_httpyac_types(source);
    let returns_map = extract_return_annotations(source);

    // Body `return { … }` when JSDoc missing (common in user modules).
    let mut returns_map = returns_map;
    for (fname, typ) in infer_returns_from_function_bodies(source, &stripped) {
        returns_map.entry(fname).or_insert(typ);
    }

    // Materialize `@returns {{ a: t, b: t }}` / body objects into named types `$ret_<fn>`.
    let mut resolved_returns: HashMap<String, String> = HashMap::new();
    for (fname, typ) in &returns_map {
        let (type_name, inline) = materialize_return_type(fname, typ, source_name);
        if let Some(tree) = inline {
            result.inline_types.insert(type_name.clone(), tree);
        }
        resolved_returns.insert(fname.clone(), type_name);
    }
    let ret = |name: &str| resolved_returns.get(name).cloned();

    // Top-level functions
    for (name, is_method) in find_functions(&stripped) {
        result.tree.insert_path(
            &name,
            ScriptMember {
                name: name.clone(),
                detail: format!("function — {source_name}"),
                documentation: format!("From `.script/{source_name}`"),
                insert: Some(format!("{name}($1)")),
                is_method,
                source: source_name.to_string(),
                returns: None,
            }
            .with_returns(ret(&name)),
        );
    }

    // exports.foo = … / module.exports.foo =
    for name in find_exports_dot_assigns(&stripped) {
        result.tree.insert_path(
            &name,
            ScriptMember {
                name: name.clone(),
                detail: format!("export — {source_name}"),
                documentation: format!("`exports.{name}` in `.script/{source_name}`"),
                insert: Some(format!("{name}($1)")),
                is_method: true,
                source: source_name.to_string(),
                returns: None,
            }
            .with_returns(ret(&name)),
        );
    }

    // module.exports = { … } nested object (use resolved return type names, e.g. $ret_signRequest)
    if let Some(obj) = extract_module_exports_object(&stripped) {
        insert_object_keys(&mut result.tree, "", &obj, source_name, &resolved_returns);
    }

    // class methods: class Foo { bar() {} }
    for (class_name, methods) in find_class_methods(&stripped) {
        for m in methods {
            let path = if class_name.is_empty() {
                m.clone()
            } else {
                format!("{class_name}.{m}")
            };
            result.tree.insert_path(
                &m,
                ScriptMember {
                    name: m.clone(),
                    detail: format!("class method — {source_name}"),
                    documentation: format!("`{class_name}.{m}` in `.script/{source_name}`"),
                    insert: Some(format!("{m}($1)")),
                    is_method: true,
                    source: source_name.to_string(),
                    returns: None,
                }
                .with_returns(ret(&m)),
            );
            let _ = path;
        }
    }

    // const name = (…) =>  / const name = function
    for name in find_const_function_bindings(&stripped) {
        if result.tree.children.contains_key(&name) {
            continue;
        }
        result.tree.insert_path(
            &name,
            ScriptMember {
                name: name.clone(),
                detail: format!("const fn — {source_name}"),
                documentation: format!("From `.script/{source_name}`"),
                insert: Some(format!("{name}($1)")),
                is_method: true,
                source: source_name.to_string(),
                returns: None,
            }
            .with_returns(ret(&name)),
        );
    }

    // Attach whole tree under each @httpyac-path
    for tag in path_tags {
        result
            .path_attachments
            .entry(tag)
            .or_insert_with(|| result.tree.clone());
    }

    // Ensure every function/export with an object @returns points at $ret_* and has inline type
    for (fname, typ) in &returns_map {
        let (type_name, inline) = materialize_return_type(fname, typ, source_name);
        if let Some(tree) = inline {
            result
                .inline_types
                .entry(type_name.clone())
                .or_insert(tree);
            if let Some(node) = result.tree.children.get_mut(fname) {
                if let Some(m) = &mut node.member {
                    m.returns = Some(type_name.clone());
                }
            }
        }
    }

    result
}

/// One field of an object/return shape: data property **or** method (mixed supported).
#[derive(Debug, Clone)]
struct ShapeField {
    name: String,
    /// Type of property, or return type of method.
    type_str: String,
    is_method: bool,
}

/// Turn a JSDoc/body return type into a catalog type name + member tree.
/// - `Hmac` → (`Hmac`, None)
/// - `{{ a: string, f(): void, g: () => number }}` → mixed props + methods
fn materialize_return_type(
    fn_name: &str,
    typ: &str,
    source_name: &str,
) -> (String, Option<MemberTree>) {
    let typ = typ.trim();
    if typ.starts_with('{') {
        let type_name = format!("$ret_{fn_name}");
        let mut tree = MemberTree::default();
        if let Some(fields) = parse_object_shape_fields(typ) {
            for f in fields {
                let is_method = f.is_method;
                let insert = if is_method {
                    Some(format!("{}($1)", f.name))
                } else {
                    None
                };
                let detail = if is_method {
                    format!("method() → {} — return of {fn_name}", f.type_str)
                } else {
                    format!("{} — return of {fn_name}", f.type_str)
                };
                let kind_doc = if is_method { "Method" } else { "Property" };
                tree.insert_path(
                    &f.name,
                    ScriptMember {
                        name: f.name.clone(),
                        detail,
                        documentation: format!(
                            "{kind_doc} `{name}` on return shape of `{fn_name}` in {source_name}",
                            name = f.name
                        ),
                        insert,
                        is_method,
                        source: source_name.to_string(),
                        returns: if f.type_str != "any"
                            && f.type_str != "void"
                            && !f.type_str.is_empty()
                        {
                            Some(f.type_str.clone())
                        } else {
                            None
                        },
                    },
                );
            }
        }
        return (type_name, Some(tree));
    }
    (typ.to_string(), None)
}

/// Parse object shape from JSDoc **or** serialized body shape.
/// Supports mixed data + functions:
/// `{ authDate: string, refresh(): void, toHeader: () => string, flag }`
fn parse_object_shape_fields(typ: &str) -> Option<Vec<ShapeField>> {
    let s = strip_jsdoc_line_prefixes(typ.trim());
    let s = s.trim();
    if !s.starts_with('{') {
        return None;
    }
    let inner = extract_balanced(&s[1..], '{', '}')?;
    let body = strip_jsdoc_line_prefixes(inner).trim().to_string();
    if body.is_empty() {
        return None;
    }
    let mut fields = Vec::new();
    for part in split_top_level_comma(&body) {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        if let Some(f) = parse_one_shape_field(part) {
            fields.push(f);
        }
    }
    if fields.is_empty() {
        None
    } else {
        Some(fields)
    }
}

/// Remove leading `*` from JSDoc continuation lines so `{ * id: string }` parses as `id`.
fn strip_jsdoc_line_prefixes(s: &str) -> String {
    s.lines()
        .map(|line| {
            let t = line.trim_start();
            if let Some(rest) = t.strip_prefix('*') {
                rest.trim_start()
            } else {
                t
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Legacy helper used by members_for_type raw `{…}` fallback.
fn parse_object_type_props(typ: &str) -> Option<Vec<(String, String)>> {
    Some(
        parse_object_shape_fields(typ)?
            .into_iter()
            .map(|f| (f.name, f.type_str))
            .collect(),
    )
}

fn parse_one_shape_field(part: &str) -> Option<ShapeField> {
    let part = part.trim();
    // Method shorthand: `name(args): Ret` / `name(): Ret` / `name()`
    // Only when `(` immediately follows a field name — NOT `name: () => T`
    // (colon property forms are handled below so arrow/function types work).
    if let Some(paren) = part.find('(') {
        let name = part[..paren].trim().trim_matches(['\'', '"', '`']);
        if is_shape_field_name(name) {
            let after_name = part[paren..].trim_start();
            if after_name.starts_with('(') {
                let params = extract_balanced(&after_name[1..], '(', ')')?;
                let after_params = after_name[1 + params.len() + 1..].trim_start();
                let ret_ty = after_params
                    .strip_prefix(':')
                    .map(|s| s.trim().to_string())
                    .unwrap_or_else(|| "any".into());
                return Some(ShapeField {
                    name: name.to_string(),
                    type_str: ret_ty,
                    is_method: true,
                });
            }
        }
        // e.g. `toJSON: () => object` — fall through to `name: type`
    }
    // `name: type` (data) or `name: () => T` / `name: Function` (method-typed prop)
    if let Some((k, v)) = part.split_once(':') {
        let name = k.trim().trim_matches(['\'', '"', '`']);
        if !is_shape_field_name(name) {
            return None;
        }
        let v = v.trim();
        let is_method = type_str_looks_like_function(v);
        return Some(ShapeField {
            name: name.to_string(),
            type_str: if is_method {
                function_return_from_type_str(v)
            } else {
                v.to_string()
            },
            is_method,
        });
    }
    // shorthand `name` or `name?`
    let name = part.trim_end_matches('?').trim();
    if is_shape_field_name(name) {
        return Some(ShapeField {
            name: name.to_string(),
            type_str: "any".into(),
            is_method: false,
        });
    }
    None
}

fn is_shape_field_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphabetic() || c == '_' || c == '$')
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$')
}

fn type_str_looks_like_function(v: &str) -> bool {
    let v = v.trim();
    let lower = v.to_ascii_lowercase();
    lower.starts_with("function")
        || lower == "fn"
        || lower.starts_with("fn(")
        || v.starts_with('(')
        || v.contains("=>")
        || lower.starts_with("callable")
}

fn function_return_from_type_str(v: &str) -> String {
    let v = v.trim();
    // (x: string) => number
    if let Some(idx) = v.rfind("=>") {
        return v[idx + 2..].trim().to_string();
    }
    // Function or function
    "any".into()
}

/// Build a type string from a real object literal (preserves methods as `name(): any`).
/// Nested objects keep mixed data+method shape recursively.
fn object_literal_to_type_string(inner: &str) -> Option<String> {
    let obj = parse_object_lit(inner);
    object_lit_to_type_string(&obj)
}

fn object_lit_to_type_string(obj: &ObjectLit) -> Option<String> {
    if obj.props.is_empty() {
        return None;
    }
    let mut parts = Vec::new();
    for (key, val) in &obj.props {
        match val {
            ObjVal::Nested(inner) => {
                if let Some(nested) = object_lit_to_type_string(inner) {
                    parts.push(format!("{key}: {nested}"));
                } else {
                    parts.push(format!("{key}: object"));
                }
            }
            ObjVal::Leaf { is_method, .. } => {
                if *is_method {
                    parts.push(format!("{key}(): any"));
                } else {
                    parts.push(format!("{key}: any"));
                }
            }
        }
    }
    Some(format!("{{{}}}", parts.join(", ")))
}

fn split_top_level_comma(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut depth = 0i32;
    let mut in_str: Option<char> = None;
    for c in s.chars() {
        if let Some(q) = in_str {
            cur.push(c);
            if c == q {
                in_str = None;
            }
            continue;
        }
        match c {
            '\'' | '"' | '`' => {
                in_str = Some(c);
                cur.push(c);
            }
            '{' | '(' | '[' => {
                depth += 1;
                cur.push(c);
            }
            '}' | ')' | ']' => {
                depth -= 1;
                cur.push(c);
            }
            ',' if depth == 0 => {
                out.push(std::mem::take(&mut cur));
            }
            _ => cur.push(c),
        }
    }
    if !cur.trim().is_empty() {
        out.push(cur);
    }
    out
}

fn insert_object_keys(
    tree: &mut MemberTree,
    prefix: &str,
    obj: &ObjectLit,
    source_name: &str,
    returns_map: &HashMap<String, String>,
) {
    for (key, val) in &obj.props {
        let path = if prefix.is_empty() {
            key.clone()
        } else {
            format!("{prefix}.{key}")
        };
        let ret = returns_map
            .get(key.as_str())
            .or_else(|| returns_map.get(path.as_str()))
            .cloned();
        match val {
            ObjVal::Nested(inner) => {
                tree.insert_path(
                    &path,
                    ScriptMember {
                        name: key.clone(),
                        detail: format!("object — {source_name}"),
                        documentation: format!("Nested object `{path}` in `.script/{source_name}`"),
                        insert: None,
                        is_method: false,
                        source: source_name.to_string(),
                        returns: None,
                    },
                );
                insert_object_keys(tree, &path, inner, source_name, returns_map);
            }
            ObjVal::Leaf {
                is_method,
                detail,
                returns: leaf_ret,
            } => {
                let returns = leaf_ret.clone().or(ret);
                tree.insert_path(
                    &path,
                    ScriptMember {
                        name: key.clone(),
                        detail: detail
                            .clone()
                            .unwrap_or_else(|| format!("member — {source_name}")),
                        documentation: format!("`{path}` in `.script/{source_name}`"),
                        insert: if *is_method {
                            Some(format!("{key}($1)"))
                        } else {
                            None
                        },
                        is_method: *is_method,
                        source: source_name.to_string(),
                        returns: None,
                    }
                    .with_returns(returns),
                );
            }
        }
    }
}

#[derive(Debug, Clone)]
struct ObjectLit {
    props: Vec<(String, ObjVal)>,
}

#[derive(Debug, Clone)]
enum ObjVal {
    Nested(ObjectLit),
    Leaf {
        is_method: bool,
        detail: Option<String>,
        returns: Option<String>,
    },
}

fn strip_js_comments(src: &str) -> String {
    let mut out = String::with_capacity(src.len());
    let chars: Vec<char> = src.chars().collect();
    let mut i = 0;
    let mut in_str: Option<char> = None;
    while i < chars.len() {
        let c = chars[i];
        if let Some(q) = in_str {
            out.push(c);
            if c == '\\' && i + 1 < chars.len() {
                out.push(chars[i + 1]);
                i += 2;
                continue;
            }
            if c == q {
                in_str = None;
            }
            i += 1;
            continue;
        }
        if c == '\'' || c == '"' || c == '`' {
            in_str = Some(c);
            out.push(c);
            i += 1;
            continue;
        }
        if c == '/' && i + 1 < chars.len() {
            if chars[i + 1] == '/' {
                i += 2;
                while i < chars.len() && chars[i] != '\n' {
                    i += 1;
                }
                continue;
            }
            if chars[i + 1] == '*' {
                i += 2;
                while i + 1 < chars.len() && !(chars[i] == '*' && chars[i + 1] == '/') {
                    // keep newlines for line-based heuristics
                    if chars[i] == '\n' {
                        out.push('\n');
                    }
                    i += 1;
                }
                i = (i + 2).min(chars.len());
                continue;
            }
        }
        out.push(c);
        i += 1;
    }
    out
}

fn extract_httpyac_paths(src: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut search = src;
    while let Some(idx) = search.find("@httpyac-path") {
        let after = &search[idx + "@httpyac-path".len()..];
        let after = after.trim_start();
        let path: String = after
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '.' || *c == '$')
            .collect();
        if !path.is_empty() {
            out.push(path);
        }
        search = &search[idx + 1..];
    }
    out
}

fn extract_httpyac_types(src: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut search = src;
    while let Some(idx) = search.find("@httpyac-type") {
        let after = &search[idx + "@httpyac-type".len()..];
        let after = after.trim_start();
        let name: String = after
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '$')
            .collect();
        if !name.is_empty() {
            out.push(name);
        }
        search = &search[idx + 1..];
    }
    out
}

/// Map function/property name → return type from JSDoc / trailing comments.
///
/// Supports:
/// - `/** @returns {Hmac} */` then `createHmac` / `createHmac() {`
/// - `createHmac() {}, // @returns {Hmac}`
/// - `createHmac() {}, // → Hmac`
fn extract_return_annotations(src: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();

    // Block JSDoc: @returns {Type} … next export/function/method name
    let mut search = src;
    while let Some(idx) = find_returns_tag(search) {
        let after_tag = &search[idx..];
        let Some((typ, consumed)) = parse_returns_type(after_tag) else {
            search = &search[idx + 1..];
            continue;
        };
        let after = after_tag[consumed..].trim_start();
        // Skip rest of comment block if still inside */
        let after = if let Some(end) = after.find("*/") {
            after[end + 2..].trim_start()
        } else {
            after
        };
        if let Some(name) = take_annotated_target_name(after) {
            if !is_reserved(&name) && !is_control_flow_or_keyword(&name) {
                map.insert(name, typ);
            }
        }
        search = &search[idx + 1..];
    }

    // Line trailing: name(...) {…}, // → Type  OR  // @returns {Type}
    for line in src.lines() {
        let (code, comment) = match line.split_once("//") {
            Some((c, rest)) => (c, rest.trim()),
            None => continue,
        };
        let typ = if let Some(t) = comment.strip_prefix("→").or_else(|| comment.strip_prefix("->"))
        {
            Some(t.trim().split_whitespace().next().unwrap_or("").to_string())
        } else if comment.contains("@returns") || comment.contains("@return") {
            parse_returns_type(comment).map(|(t, _)| t)
        } else {
            None
        };
        let Some(typ) = typ.filter(|t| !t.is_empty()) else {
            continue;
        };
        // Prefer exports.foo / module.exports.foo on the code side
        if let Some(name) = take_annotated_target_name(code) {
            if !is_reserved(&name) && !is_control_flow_or_keyword(&name) {
                map.insert(name, typ);
                continue;
            }
        }
        // find last ident before ( on the code side
        if let Some(paren) = code.rfind('(') {
            let before = code[..paren].trim_end();
            let name: String = before
                .chars()
                .rev()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '$')
                .collect::<String>()
                .chars()
                .rev()
                .collect();
            if !name.is_empty() && !is_reserved(&name) {
                map.insert(name, typ);
            }
        }
    }

    map
}

/// Name that a `@returns` annotation applies to:
/// `function foo`, `foo()`, `exports.foo =`, `module.exports.foo =`,
/// `const foo =`, `export function foo`, method shorthand `foo(`.
fn take_annotated_target_name(after: &str) -> Option<String> {
    let mut s = after.trim_start();
    // module.exports.NAME  /  exports.NAME
    for prefix in ["module.exports.", "exports."] {
        if let Some(rest) = s.strip_prefix(prefix) {
            return take_ident(rest.trim_start());
        }
    }
    // Strip keywords only at word boundaries (do not turn `getFoo` into `Foo`).
    for _ in 0..8 {
        let mut stripped = false;
        for kw in [
            "export", "default", "async", "function", "const", "let", "var", "static", "get", "set",
        ] {
            if let Some(rest) = strip_keyword_prefix(s, kw) {
                s = rest;
                stripped = true;
                break;
            }
        }
        if !stripped {
            break;
        }
    }
    take_ident(s)
}

/// `strip_prefix(kw)` only when followed by whitespace, `(`, or end.
fn strip_keyword_prefix<'a>(s: &'a str, kw: &str) -> Option<&'a str> {
    let rest = s.strip_prefix(kw)?;
    if rest.is_empty()
        || rest.starts_with(|c: char| c.is_whitespace() || c == '(' || c == '*')
    {
        Some(rest.trim_start())
    } else {
        None
    }
}

fn find_returns_tag(s: &str) -> Option<usize> {
    let a = s.find("@returns");
    let b = s.find("@return");
    match (a, b) {
        (Some(i), Some(j)) => Some(i.min(j)),
        (Some(i), None) => Some(i),
        (None, Some(j)) => Some(j),
        _ => None,
    }
}

/// Parse `@returns {Type}` or `@return Type` at start of string → (type, bytes_consumed_approx).
/// Supports nested object types: `@returns {{ authDate: string, authentication: string }}`.
fn parse_returns_type(s: &str) -> Option<(String, usize)> {
    let s_trim = s.trim_start();
    let start_offset = s.len() - s_trim.len();
    let (tag_len, rest) = if s_trim.starts_with("@returns") {
        ("@returns".len(), s_trim["@returns".len()..].trim_start())
    } else if s_trim.starts_with("@return") {
        // Prefer longer tag; "@returns" already handled above
        ("@return".len(), s_trim["@return".len()..].trim_start())
    } else {
        return None;
    };

    if rest.starts_with("{{") {
        // JSDoc object: @returns {{ authDate: string, authentication: string }}
        let props = extract_balanced(&rest[2..], '{', '}')?;
        let typ = format!("{{{props}}}");
        let leading = s_trim.len() - rest.len();
        // `{{` + props + `}}`
        let consumed = start_offset + leading + 2 + props.len() + 2;
        return Some((typ, consumed.max(1)));
    }
    if rest.starts_with('{') {
        // `{Hmac}` or `{ a: string, f(): void }` (single brace object / mixed)
        let inner = extract_balanced(&rest[1..], '{', '}')?;
        let inner_t = inner.trim();
        // Object shape if it has fields (colon, comma, or method `name(`); else bare type name.
        let typ = if inner_t.contains(':')
            || inner_t.contains(',')
            || inner_t.contains('(')
        {
            format!("{{{inner_t}}}")
        } else {
            // `{Hmac}` → Hmac
            inner_t.to_string()
        };
        if typ.is_empty() {
            return None;
        }
        let leading = s_trim.len() - rest.len();
        let consumed = start_offset + leading + 1 + inner.len() + 1;
        return Some((typ, consumed.max(1)));
    }
    // bare @returns Hmac
    let typ: String = rest
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '|' || *c == '[' || *c == ']')
        .collect();
    if typ.is_empty() {
        return None;
    }
    Some((typ, start_offset + tag_len + 1))
}

fn find_functions(src: &str) -> Vec<(String, bool)> {
    let mut out = Vec::new();
    let mut i = 0;
    let bytes = src.as_bytes();
    while i < bytes.len() {
        // async function name | function name
        let rest = &src[i..];
        let (adv, name) = if rest.starts_with("async function") {
            let after = rest["async function".len()..].trim_start();
            (rest.len() - after.len(), take_ident(after))
        } else if rest.starts_with("function") {
            let after = rest["function".len()..].trim_start();
            // skip if function( anonymous
            if after.starts_with('(') {
                (8, None)
            } else {
                (rest.len() - after.len(), take_ident(after))
            }
        } else {
            (1, None)
        };
        if let Some(n) = name {
            if !is_reserved(&n) {
                out.push((n, true));
            }
        }
        i += adv.max(1);
    }
    out
}

fn find_exports_dot_assigns(src: &str) -> Vec<String> {
    let mut out = Vec::new();
    for marker in ["module.exports.", "exports."] {
        let mut search = src;
        while let Some(idx) = search.find(marker) {
            let after = &search[idx + marker.len()..];
            if let Some(name) = take_ident(after) {
                if !is_reserved(&name) && name != "exports" {
                    out.push(name);
                }
            }
            search = &search[idx + marker.len()..];
        }
    }
    out.sort();
    out.dedup();
    out
}

fn find_const_function_bindings(src: &str) -> Vec<String> {
    let mut out = Vec::new();
    for kw in ["const ", "let ", "var "] {
        let mut search = src;
        while let Some(idx) = search.find(kw) {
            let after = &search[idx + kw.len()..];
            if let Some(name) = take_ident(after) {
                let rest = after[name.len()..].trim_start();
                if let Some(rest) = rest.strip_prefix('=') {
                    let rest = rest.trim_start();
                    if rest.starts_with("function")
                        || rest.starts_with("async")
                        || rest.starts_with('(')
                        || rest.starts_with("async (")
                    {
                        if !is_reserved(&name) {
                            out.push(name);
                        }
                    }
                }
            }
            search = &search[idx + kw.len()..];
        }
    }
    out.sort();
    out.dedup();
    out
}

fn find_class_methods(src: &str) -> Vec<(String, Vec<String>)> {
    let mut out = Vec::new();
    let mut search = src;
    while let Some(idx) = search.find("class ") {
        let after = search[idx + 6..].trim_start();
        let class_name = take_ident(after).unwrap_or_default();
        // find body {
        let Some(brace) = after.find('{') else {
            search = &search[idx + 6..];
            continue;
        };
        let body_start = &after[brace + 1..];
        let Some(body) = extract_balanced(body_start, '{', '}') else {
            search = &search[idx + 6..];
            continue;
        };
        let methods = scan_class_body_methods(body);
        if !methods.is_empty() {
            out.push((class_name, methods));
        }
        search = &search[idx + 6..];
    }
    out
}

fn scan_class_body_methods(body: &str) -> Vec<String> {
    let mut methods = Vec::new();
    // Rough: ident( at line starts / after ;
    let mut i = 0;
    let b = body.as_bytes();
    while i < b.len() {
        // skip whitespace
        while i < b.len() && b[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= b.len() {
            break;
        }
        // skip async
        let rest = &body[i..];
        let rest = if rest.starts_with("async") {
            let r = rest[5..].trim_start();
            i += rest.len() - r.len();
            r
        } else {
            rest
        };
        // skip get/set/static
        let rest = {
            let mut r = rest;
            for p in ["static ", "get ", "set ", "#"] {
                if r.starts_with(p) {
                    r = r[p.len()..].trim_start();
                }
            }
            r
        };
        if let Some(name) = take_ident(rest) {
            let after_name = rest[name.len()..].trim_start();
            if after_name.starts_with('(') && !is_reserved(&name) && name != "constructor" {
                methods.push(name);
            }
        }
        // advance to next ; or newline-ish boundary
        if let Some(rel) = body[i..].find([';', '\n', '}']) {
            i += rel + 1;
        } else {
            break;
        }
    }
    methods.sort();
    methods.dedup();
    methods
}

fn extract_module_exports_object(src: &str) -> Option<ObjectLit> {
    // module.exports = { … }  or  module.exports={…
    let markers = ["module.exports=", "module.exports ="];
    for m in markers {
        if let Some(idx) = src.find(m) {
            let after = src[idx + m.len()..].trim_start();
            // skip `module.exports = something` non-object
            if !after.starts_with('{') {
                // try next occurrence via simple continue — only first
                continue;
            }
            let inner = extract_balanced(&after[1..], '{', '}')?;
            return Some(parse_object_lit(inner));
        }
    }
    // exports = { is uncommon; also support module.exports = Object.freeze({
    if let Some(idx) = src.find("module.exports") {
        let after = &src[idx..];
        if let Some(brace) = after.find('{') {
            // only if = appears before {
            let between = &after["module.exports".len()..brace];
            if between.contains('=') {
                let inner = extract_balanced(&after[brace + 1..], '{', '}')?;
                return Some(parse_object_lit(inner));
            }
        }
    }
    None
}

fn parse_object_lit(inner: &str) -> ObjectLit {
    let mut props = Vec::new();
    let mut i = 0;
    let chars: Vec<char> = inner.chars().collect();
    while i < chars.len() {
        // skip ws and commas
        while i < chars.len() && (chars[i].is_whitespace() || chars[i] == ',') {
            i += 1;
        }
        if i >= chars.len() {
            break;
        }
        // key: ident or 'str' or "str"
        let (key, new_i) = match take_object_key(&chars, i) {
            Some(v) => v,
            None => break,
        };
        i = new_i;
        while i < chars.len() && chars[i].is_whitespace() {
            i += 1;
        }
        // shorthand { foo } or { foo, } — data property (not a method)
        if i >= chars.len() || chars[i] == ',' || chars[i] == '}' {
            props.push((
                key,
                ObjVal::Leaf {
                    is_method: false,
                    detail: Some("property".into()),
                    returns: None,
                },
            ));
            continue;
        }
        if chars[i] != ':' {
            // method shorthand: foo(a,b) { or foo() {
            if chars[i] == '(' {
                props.push((
                    key,
                    ObjVal::Leaf {
                        is_method: true,
                        detail: Some("method".into()),
                        returns: None,
                    },
                ));
                // skip to end of method body roughly
                i = skip_method_or_value(&chars, i);
                continue;
            }
            i += 1;
            continue;
        }
        i += 1; // skip :
        while i < chars.len() && chars[i].is_whitespace() {
            i += 1;
        }
        if i >= chars.len() {
            break;
        }
        if chars[i] == '{' {
            let s: String = chars[i + 1..].iter().collect();
            if let Some(body) = extract_balanced(&s, '{', '}') {
                props.push((key, ObjVal::Nested(parse_object_lit(body))));
                // advance past { body }
                i += 1 + body.len() + 1;
                continue;
            }
        }
        // function / arrow / other leaf
        let is_method = chars[i] == '('
            || slice_starts_with(&chars, i, "function")
            || slice_starts_with(&chars, i, "async");
        props.push((
            key,
            ObjVal::Leaf {
                is_method,
                detail: None,
                returns: None,
            },
        ));
        i = skip_method_or_value(&chars, i);
    }
    ObjectLit { props }
}

fn take_object_key(chars: &[char], mut i: usize) -> Option<(String, usize)> {
    while i < chars.len() && chars[i].is_whitespace() {
        i += 1;
    }
    if i >= chars.len() {
        return None;
    }
    if chars[i] == '\'' || chars[i] == '"' {
        let q = chars[i];
        i += 1;
        let mut s = String::new();
        while i < chars.len() && chars[i] != q {
            s.push(chars[i]);
            i += 1;
        }
        if i < chars.len() {
            i += 1;
        }
        return Some((s, i));
    }
    // ident
    if !chars[i].is_ascii_alphanumeric() && chars[i] != '_' && chars[i] != '$' {
        return None;
    }
    let mut s = String::new();
    while i < chars.len() && (chars[i].is_ascii_alphanumeric() || chars[i] == '_' || chars[i] == '$')
    {
        s.push(chars[i]);
        i += 1;
    }
    Some((s, i))
}

fn skip_method_or_value(chars: &[char], mut i: usize) -> usize {
    // Skip until top-level comma or end, respecting braces/parens/strings
    let mut depth_brace = 0i32;
    let mut depth_paren = 0i32;
    let mut depth_bracket = 0i32;
    let mut in_str: Option<char> = None;
    let start = i;
    while i < chars.len() {
        let c = chars[i];
        if let Some(q) = in_str {
            if c == '\\' && i + 1 < chars.len() {
                i += 2;
                continue;
            }
            if c == q {
                in_str = None;
            }
            i += 1;
            continue;
        }
        match c {
            '\'' | '"' | '`' => in_str = Some(c),
            '{' => depth_brace += 1,
            '}' => {
                depth_brace -= 1;
                if depth_brace < 0 {
                    return i;
                }
            }
            '(' => depth_paren += 1,
            ')' => depth_paren -= 1,
            '[' => depth_bracket += 1,
            ']' => depth_bracket -= 1,
            ',' if depth_brace == 0 && depth_paren == 0 && depth_bracket == 0 && i > start => {
                return i;
            }
            _ => {}
        }
        i += 1;
    }
    i
}

fn slice_starts_with(chars: &[char], i: usize, s: &str) -> bool {
    let s: Vec<char> = s.chars().collect();
    if i + s.len() > chars.len() {
        return false;
    }
    chars[i..i + s.len()] == s[..]
}

fn extract_balanced<'a>(s: &'a str, open: char, close: char) -> Option<&'a str> {
    let mut depth = 1i32;
    let mut in_str: Option<char> = None;
    let chars: Vec<(usize, char)> = s.char_indices().collect();
    for &(idx, c) in &chars {
        if let Some(q) = in_str {
            if c == '\\' {
                continue;
            }
            if c == q {
                in_str = None;
            }
            continue;
        }
        if c == '\'' || c == '"' || c == '`' {
            in_str = Some(c);
            continue;
        }
        if c == open {
            depth += 1;
        } else if c == close {
            depth -= 1;
            if depth == 0 {
                return Some(&s[..idx]);
            }
        }
    }
    None
}

fn take_ident(s: &str) -> Option<String> {
    let s = s.trim_start();
    let mut chars = s.chars();
    let first = chars.next()?;
    if !(first.is_ascii_alphabetic() || first == '_' || first == '$') {
        return None;
    }
    let mut name = String::new();
    name.push(first);
    for c in chars {
        if c.is_ascii_alphanumeric() || c == '_' || c == '$' {
            name.push(c);
        } else {
            break;
        }
    }
    Some(name)
}

fn is_reserved(name: &str) -> bool {
    matches!(
        name,
        "if" | "else"
            | "for"
            | "while"
            | "do"
            | "switch"
            | "case"
            | "break"
            | "continue"
            | "return"
            | "function"
            | "var"
            | "let"
            | "const"
            | "class"
            | "new"
            | "this"
            | "super"
            | "typeof"
            | "instanceof"
            | "void"
            | "delete"
            | "try"
            | "catch"
            | "finally"
            | "throw"
            | "with"
            | "debugger"
            | "default"
            | "import"
            | "export"
            | "from"
            | "as"
            | "async"
            | "await"
            | "yield"
            | "of"
            | "in"
            | "true"
            | "false"
            | "null"
            | "undefined"
    )
}

/// Script text for require/type inference around the cursor.
///
/// Prefers the full `{{ … }}` island (including lines **below** the cursor) so
/// `const signed = signRequest(…)` still types `signed` when completing on an
/// earlier line, and chained tips see the whole pre-request block.
pub fn script_window_text(lines: &[&str], line_idx: usize) -> String {
    let block_start = (0..=line_idx)
        .rev()
        .find(|&i| lines[i].trim_start().starts_with("###"))
        .map(|i| i + 1)
        .unwrap_or(0);
    let block_end = ((line_idx + 1)..lines.len())
        .find(|&i| lines[i].trim_start().starts_with("###"))
        .unwrap_or(lines.len());

    // Narrow to enclosing {{ … }} when present
    let mut start = block_start;
    let mut end = block_end;
    if let Some(open) = (block_start..=line_idx.min(lines.len().saturating_sub(1)))
        .rev()
        .find(|&i| {
            let t = lines[i].trim();
            t == "{{" || t.starts_with("{{")
        })
    {
        start = open;
        if let Some(close) = ((line_idx + 1)..block_end).find(|&i| {
            let t = lines[i].trim();
            t == "}}" || t.ends_with("}}")
        }) {
            end = close + 1;
        }
    }

    // Also include `> {% … %}` handler window
    if let Some(open) = (block_start..=line_idx.min(lines.len().saturating_sub(1)))
        .rev()
        .find(|&i| lines[i].contains("{%"))
    {
        start = start.min(open);
        if let Some(close) = (line_idx..block_end).find(|&i| lines[i].contains("%}")) {
            end = end.max(close + 1);
        }
    }

    if start >= end {
        let end = (line_idx + 1).min(lines.len());
        return lines[block_start..end].join("\n");
    }
    lines[start..end].join("\n")
}

/// Builtin module id → static props table name mapping used by main.
pub fn builtin_module_props(module: &str) -> Option<&'static [(&'static str, &'static str)]> {
    use crate::completions::{
        BUFFER_STATIC_PROPS, CRYPTO_PROPS, FS_PROPS, PATH_PROPS,
    };
    match module {
        "crypto" => Some(CRYPTO_PROPS),
        "fs" => Some(FS_PROPS),
        "path" => Some(PATH_PROPS),
        "buffer" | "Buffer" => Some(BUFFER_STATIC_PROPS),
        _ => None,
    }
}

/// **Phenomenon (not special cases):** after a binding gets a *shape* from the RHS,
/// `name.` completes that shape's members.
///
/// Shape sources (any names):
/// - call whose callee has `@returns` / body `return {…}` / known fluent API
/// - object literal `{ a, b: 1 }`
/// - `new Ctor(...)` (limited)
/// - destructuring `const { a, b } = <shapedExpr>` → field types for `a`/`b`
///
/// Registers ephemeral types on `catalog` so later `name.` resolves.
pub fn parse_typed_bindings(
    text: &str,
    catalog: &mut ScriptCatalog,
    require_bindings: &[RequireBinding],
) -> HashMap<String, String> {
    let mut vars: HashMap<String, String> = HashMap::new();
    for kw in ["const ", "let ", "var "] {
        let mut search = text;
        while let Some(idx) = search.find(kw) {
            let after = &search[idx + kw.len()..];
            let after_trim = after.trim_start();

            // const { a, b } = <expr>  (not require — that is parse_require_bindings)
            if after_trim.starts_with('{') {
                if let Some(end_brace) = find_matching_brace(after_trim) {
                    let destructure = &after_trim[..=end_brace];
                    let rest = after_trim[end_brace + 1..].trim_start();
                    if let Some(rest) = rest.strip_prefix('=') {
                        let expr = take_assignment_expr(rest.trim_start());
                        if !expr.starts_with("require(") {
                            if let Some(shape) =
                                infer_expr_type(expr, catalog, require_bindings, &vars)
                            {
                                let shape = ensure_shape_type(catalog, &shape, "destructure");
                                bind_destructure_fields(
                                    &mut vars,
                                    catalog,
                                    destructure,
                                    &shape,
                                );
                            }
                        }
                    }
                }
                search = &search[idx + kw.len()..];
                continue;
            }
            if after_trim.starts_with('[') {
                search = &search[idx + kw.len()..];
                continue;
            }

            if let Some(name) = take_ident(after) {
                let rest = after[name.len()..].trim_start();
                if let Some(rest) = rest.strip_prefix('=') {
                    let rest = rest.trim_start();
                    let expr = take_assignment_expr(rest);
                    if name.is_empty() || is_reserved(&name) || expr.starts_with("require(")
                    {
                        search = &search[idx + kw.len()..];
                        continue;
                    }
                    if let Some(ty) = infer_binding_type(
                        catalog,
                        require_bindings,
                        &vars,
                        &name,
                        expr,
                    ) {
                        vars.insert(name, ty);
                    }
                }
            }
            search = &search[idx + kw.len()..];
        }
    }
    // Bare assignments `name = expr`
    let mut search = text;
    while let Some(idx) = search.find('\n') {
        let line = search[..idx].trim();
        search = &search[idx + 1..];
        if line.starts_with("const ") || line.starts_with("let ") || line.starts_with("var ") {
            continue;
        }
        if let Some(eq) = line.find('=') {
            let left = line[..eq].trim();
            if left.contains('(') || left.contains('.') || left.starts_with('{') {
                continue;
            }
            if let Some(name) = take_ident(left) {
                if name.len() == left.len() && !is_reserved(&name) {
                    let expr = take_assignment_expr(line[eq + 1..].trim());
                    if let Some(ty) = infer_binding_type(
                        catalog,
                        require_bindings,
                        &vars,
                        &name,
                        expr,
                    ) {
                        vars.insert(name, ty);
                    }
                }
            }
        }
    }
    vars
}

/// Unified: expr → canonical type id present in `catalog.types` when shape-like.
fn infer_binding_type(
    catalog: &mut ScriptCatalog,
    require_bindings: &[RequireBinding],
    vars: &HashMap<String, String>,
    hint_name: &str,
    expr: &str,
) -> Option<String> {
    let expr = expr.trim();
    if expr.starts_with('{') {
        return register_object_literal_type(catalog, hint_name, expr);
    }
    if expr.starts_with("new ") {
        if let Some(ty) = infer_new_expr_type(expr) {
            return Some(ensure_shape_type(catalog, &ty, hint_name));
        }
    }
    let ty = infer_expr_type(expr, catalog, require_bindings, vars)?;
    Some(ensure_shape_type(catalog, &ty, hint_name))
}

/// Ensure object-shaped / synthetic types are registered so `members_for_type` works.
/// Named types (`Hmac`) and primitives pass through.
pub fn ensure_shape_type(catalog: &mut ScriptCatalog, ty: &str, hint_name: &str) -> String {
    let ty = normalize_type_name(ty);
    if ty.is_empty() || is_primitive_type(&ty) {
        return ty;
    }
    if catalog.types.contains_key(&ty) {
        return ty;
    }
    if ty.starts_with('$') {
        // expected synthetic id — empty type still ok
        catalog
            .types
            .entry(ty.clone())
            .or_insert_with(MemberTree::default);
        return ty;
    }
    if ty.starts_with('{') {
        let (id, tree) = materialize_return_type(hint_name, &ty, "infer");
        if let Some(tree) = tree {
            catalog
                .types
                .entry(id.clone())
                .or_insert_with(MemberTree::default)
                .merge_override(&tree);
        }
        return id;
    }
    // Unknown named type: still return name (may gain members later)
    ty
}

fn find_matching_brace(s: &str) -> Option<usize> {
    if !s.starts_with('{') {
        return None;
    }
    let inner = extract_balanced(&s[1..], '{', '}')?;
    Some(1 + inner.len()) // index of closing `}`
}

/// `const { a, b: c } = shape` → bind locals to field types (or `any` / nested shape).
fn bind_destructure_fields(
    vars: &mut HashMap<String, String>,
    catalog: &mut ScriptCatalog,
    destructure: &str,
    shape_ty: &str,
) {
    let names = parse_destructure_names(destructure);
    // Map original export key → local name for `b: c`
    let pairs = parse_destructure_key_local_pairs(destructure);
    let field_types: HashMap<String, String> = catalog
        .members_for_type(shape_ty, "")
        .into_iter()
        .map(|m| {
            (
                m.name.clone(),
                m.returns
                    .clone()
                    .unwrap_or_else(|| "any".into()),
            )
        })
        .collect();

    if !pairs.is_empty() {
        for (key, local) in pairs {
            let ft = field_types
                .get(&key)
                .cloned()
                .unwrap_or_else(|| "any".into());
            let ft = ensure_shape_type(catalog, &ft, &local);
            if !is_primitive_type(&ft) || catalog.types.contains_key(&ft) {
                vars.insert(local, ft);
            } else {
                // primitive field — still record for documentation; no members
                vars.insert(local, ft);
            }
        }
        return;
    }
    for local in names {
        let ft = field_types
            .get(&local)
            .cloned()
            .unwrap_or_else(|| "any".into());
        let ft = ensure_shape_type(catalog, &ft, &local);
        vars.insert(local, ft);
    }
}

fn parse_destructure_key_local_pairs(left: &str) -> Vec<(String, String)> {
    let inner = left
        .trim()
        .strip_prefix('{')
        .and_then(|s| {
            let end = find_matching_brace(left)?;
            Some(&left[1..end])
        })
        .unwrap_or(left);
    let mut pairs = Vec::new();
    for part in inner.split(',') {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        if let Some((k, v)) = part.split_once(':') {
            let key = k.trim().to_string();
            let local = v
                .trim()
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '$')
                .collect::<String>();
            if !key.is_empty() && !local.is_empty() {
                pairs.push((key, local));
            }
        } else if let Some((k, v)) = part.split_once(" as ") {
            let key = k.trim().to_string();
            let local = v.trim().to_string();
            if !key.is_empty() && !local.is_empty() {
                pairs.push((key, local));
            }
        }
        // bare names handled by parse_destructure_names path
    }
    pairs
}

/// Expression after `=` until top-level `;` (allows nested parens/braces/newlines).
fn take_assignment_expr(rest: &str) -> &str {
    let chars: Vec<char> = rest.chars().collect();
    let mut i = 0;
    let mut depth_paren = 0i32;
    let mut depth_brace = 0i32;
    let mut depth_brack = 0i32;
    let mut in_str: Option<char> = None;
    while i < chars.len() {
        let c = chars[i];
        if let Some(q) = in_str {
            if c == '\\' && i + 1 < chars.len() {
                i += 2;
                continue;
            }
            if c == q {
                in_str = None;
            }
            i += 1;
            continue;
        }
        match c {
            '\'' | '"' | '`' => in_str = Some(c),
            '(' => depth_paren += 1,
            ')' => depth_paren -= 1,
            '{' => depth_brace += 1,
            '}' => depth_brace -= 1,
            '[' => depth_brack += 1,
            ']' => depth_brack -= 1,
            ';' if depth_paren == 0 && depth_brace == 0 && depth_brack == 0 => {
                break;
            }
            _ => {}
        }
        i += 1;
    }
    // byte index from char index
    rest.char_indices()
        .nth(i)
        .map(|(b, _)| rest[..b].trim())
        .unwrap_or_else(|| rest.trim())
}

fn register_object_literal_type(
    catalog: &mut ScriptCatalog,
    var_name: &str,
    expr: &str,
) -> Option<String> {
    let expr = expr.trim();
    if !expr.starts_with('{') {
        return None;
    }
    let inner = extract_balanced(&expr[1..], '{', '}')?;
    let ty = object_literal_to_type_string(inner)?;
    let type_name = format!("$obj_{var_name}");
    let (_id, tree) = materialize_return_type(var_name, &ty, "$script");
    if let Some(tree) = tree {
        catalog
            .types
            .entry(type_name.clone())
            .or_insert_with(MemberTree::default)
            .merge_override(&tree);
        Some(type_name)
    } else {
        None
    }
}

fn infer_new_expr_type(expr: &str) -> Option<String> {
    let rest = expr.trim().strip_prefix("new")?.trim_start();
    let name = take_ident(rest)?;
    match name.as_str() {
        "Date" => Some("Date".into()), // static Date. in modules; instance tips sparse
        "Map" | "Set" | "Error" | "RegExp" | "Promise" => Some(name),
        _ => Some(name),
    }
}

/// Infer type of a JS expression using catalog return types + prior var types.
///
/// Handles chains: `crypto.createHmac('sha256', k).update(data)`
pub fn infer_expr_type(
    expr: &str,
    catalog: &ScriptCatalog,
    require_bindings: &[RequireBinding],
    vars: &HashMap<String, String>,
) -> Option<String> {
    let expr = expr.trim().trim_end_matches(';').trim();
    if expr.is_empty() {
        return None;
    }
    // Strip balanced calls from the right repeatedly while resolving left chain.
    // Tokenize into segments: ident or ident(args)
    let segments = split_call_chain(expr)?;
    if segments.is_empty() {
        return None;
    }

    // Head: module, require-alias, destructured export, or typed var
    let head = &segments[0];
    let head_name = head.name.as_str();
    let mut current_type: Option<String> = None;
    let mut module_path = String::new();

    if let Some(ty) = vars.get(head_name) {
        current_type = Some(ty.clone());
    } else if let Some(b) = require_bindings.iter().find(|b| b.name == head_name) {
        if head.called {
            // const { signRequest } = require(...); signRequest(...)
            let ret_path = if b.maps_to_module {
                b.module.clone()
            } else {
                format!("{}.{}", b.module, b.name)
            };
            if let Some(r) = catalog.returns_of_path(&ret_path) {
                current_type = Some(normalize_type_name(&r));
            }
        } else if b.maps_to_module {
            module_path = b.module.clone();
        }
    } else {
        module_path = head_name.to_string();
        if head.called {
            // local function / any loaded export
            if let Some(r) = catalog.returns_of_path(&format!("$script.{head_name}")) {
                current_type = Some(normalize_type_name(&r));
                module_path.clear();
            } else if let Some(r) = catalog.find_returns_for_export(head_name) {
                current_type = Some(normalize_type_name(&r));
                module_path.clear();
            } else if let Some(r) = catalog.returns_of_path(head_name) {
                current_type = Some(normalize_type_name(&r));
                module_path.clear();
            }
        }
    }

    for seg in segments.iter().skip(1) {
        if let Some(ref ty) = current_type {
            // Member on a type instance
            let member_path = seg.name.as_str();
            if let Some(tree) = catalog.types.get(ty) {
                if let Some(ret) = member_returns_at(tree, member_path) {
                    if seg.called || true {
                        // property access without call still may be nested object on type — rare
                        current_type = if seg.called {
                            Some(ret)
                        } else if catalog.types.contains_key(&ret) {
                            Some(ret)
                        } else {
                            // non-call: if has children under type, stay; else use returns if any
                            Some(ret)
                        };
                    }
                } else if seg.called {
                    // unknown method — clear
                    current_type = None;
                    break;
                }
            } else {
                current_type = None;
                break;
            }
        } else if !module_path.is_empty() {
            module_path = format!("{module_path}.{}", seg.name);
            if seg.called {
                if let Some(r) = catalog.returns_of_path(&module_path) {
                    current_type = Some(normalize_type_name(&r));
                    module_path.clear();
                } else {
                    // call with no known returns
                    current_type = None;
                    module_path.clear();
                    break;
                }
            }
        } else {
            break;
        }
    }

    // If we ended on a module path without call (const x = crypto) — not a type
    current_type.map(|t| normalize_type_name(&t))
}

fn normalize_type_name(t: &str) -> String {
    let t = unwrap_promise_type(t.trim());
    // Object shape → keep full `{ … }` so members_for_type can parse props,
    // but prefer already-materialized `$ret_*` when present in string form.
    if t.starts_with('{') {
        return t.to_string();
    }
    // Prefer first non-primitive in unions
    for part in t.split('|') {
        let p = part.trim().trim_end_matches("[]");
        let p = unwrap_promise_type(p);
        if !p.is_empty() && !is_primitive_type(p) {
            return p.to_string();
        }
    }
    t.split('|').next().unwrap_or(t).trim().to_string()
}

#[derive(Debug)]
struct ChainSeg {
    name: String,
    called: bool,
}

/// Split `crypto.createHmac('a', b).update(x)` into segments with call flags.
fn split_call_chain(expr: &str) -> Option<Vec<ChainSeg>> {
    let mut segs = Vec::new();
    let chars: Vec<char> = expr.chars().collect();
    let mut i = 0;
    // optional leading new?
    while i < chars.len() && chars[i].is_whitespace() {
        i += 1;
    }
    while i < chars.len() {
        while i < chars.len() && (chars[i].is_whitespace() || chars[i] == '.') {
            i += 1;
        }
        if i >= chars.len() {
            break;
        }
        // ident
        if !(chars[i].is_ascii_alphabetic() || chars[i] == '_' || chars[i] == '$') {
            break;
        }
        let start = i;
        i += 1;
        while i < chars.len()
            && (chars[i].is_ascii_alphanumeric() || chars[i] == '_' || chars[i] == '$')
        {
            i += 1;
        }
        let name: String = chars[start..i].iter().collect();
        while i < chars.len() && chars[i].is_whitespace() {
            i += 1;
        }
        let mut called = false;
        if i < chars.len() && chars[i] == '(' {
            // skip balanced parens
            let rest: String = chars[i + 1..].iter().collect();
            if let Some(inner) = extract_balanced(&rest, '(', ')') {
                i += 1 + inner.len() + 1;
                called = true;
            } else {
                break;
            }
        }
        segs.push(ChainSeg { name, called });
    }
    if segs.is_empty() {
        None
    } else {
        Some(segs)
    }
}

/// Resolve completion path using var types: `h` / `h.update` when h: Hmac.
///
/// Returns either rewritten path for `members_for_path` **or** a type name
/// for `members_for_type` via the `PathResolve` enum.
#[derive(Debug, Clone)]
pub enum PathResolve {
    /// Normal module/path in catalog
    ModulePath(String),
    /// Instance type (show type members). `rest` is sub-path on the type after the var.
    Type { type_name: String, rest: String },
}

pub fn resolve_completion_path(
    path: &str,
    catalog: &ScriptCatalog,
    require_bindings: &[RequireBinding],
    var_types: &HashMap<String, String>,
) -> PathResolve {
    if path.is_empty() {
        return PathResolve::ModulePath(String::new());
    }
    let path = resolve_path_with_bindings(path, require_bindings);
    let mut parts = path.splitn(2, '.');
    let head = parts.next().unwrap_or("");
    let rest = parts.next().unwrap_or("").to_string();

    if let Some(ty) = var_types.get(head) {
        // h or h.update — if rest empty, type root; if rest is further access on value
        // For completing `h.` → Type Hmac rest ""
        // For completing after `h.update().` the path might be weird; chain filter handles fluent.
        // If path is `h.update` without call (user typing h.update|) members of Hmac filtered to update
        // actually dotted_prefix gives path=h filter=update — rest empty, filter separate.
        // path h.update means completing members OF update result — rare mid-path.
        if rest.is_empty() {
            return PathResolve::Type {
                type_name: ty.clone(),
                rest: String::new(),
            };
        }
        // path h.something — members under type at something? 
        // If something is a method being completed as path receiver for next: treat rest as type subpath
        // Check if rest's first segment returns a type for deeper: h.update → if we need members of update's return, path would be after call.
        // For `h.update` as path with filter "", show nothing useful from type at "update" children — update is leaf.
        // Use type members at "" only when rest empty; when rest non-empty try type.rest as nested (unlikely)
        if let Some(tree) = catalog.types.get(ty) {
            if tree.children.contains_key(rest.split('.').next().unwrap_or("")) {
                // completing nested on type (rare)
                return PathResolve::Type {
                    type_name: ty.clone(),
                    rest,
                };
            }
            // Maybe rest is a call chain result type: look up returns of first method
            if let Some(ret) = member_returns_at(tree, rest.split('.').next().unwrap_or("")) {
                let ret = normalize_type_name(&ret);
                if catalog.types.contains_key(&ret) {
                    let after = rest.splitn(2, '.').nth(1).unwrap_or("").to_string();
                    return PathResolve::Type {
                        type_name: ret,
                        rest: after,
                    };
                }
            }
        }
        return PathResolve::Type {
            type_name: ty.clone(),
            rest: String::new(),
        };
    }

    PathResolve::ModulePath(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn parse_require_bindings_basic() {
        let text = r#"
            const c = require('crypto');
            let fs = require("fs");
            var p = require('node:path');
            const x = require('./.script/helpers');
        "#;
        let b = parse_require_bindings(text);
        assert!(b.iter().any(|x| x.name == "c" && x.module == "crypto"));
        assert!(b.iter().any(|x| x.name == "fs" && x.module == "fs"));
        assert!(b.iter().any(|x| x.name == "p" && x.module == "path"));
        assert!(b.iter().any(|x| x.name == "x" && x.module == "helpers"));
    }

    #[test]
    fn resolve_alias_path() {
        let b = vec![RequireBinding {
            name: "c".into(),
            module: "crypto".into(),
            raw_path: None,
            maps_to_module: true,
        }];
        assert_eq!(resolve_path_with_bindings("c", &b), "crypto");
        assert_eq!(
            resolve_path_with_bindings("c.createHmac", &b),
            "crypto.createHmac"
        );
        assert_eq!(resolve_path_with_bindings("other", &b), "other");
        // destructure must not map whole module
        let d = vec![RequireBinding {
            name: "signRequest".into(),
            module: "auth-sign".into(),
            raw_path: Some("./scripts/auth-sign.js".into()),
            maps_to_module: false,
        }];
        assert_eq!(
            resolve_path_with_bindings("signRequest", &d),
            "signRequest"
        );
    }

    /// Matrix: exports + returns cover **single** named types and **mixed** shapes,
    /// via JSDoc and body `return`, across function / module.exports / exports.x forms.
    #[test]
    fn exports_returns_single_and_mixed_matrix() {
        // --- 1) Top-level function + module.exports = { name } ---
        let src = r#"
          /**
           * @returns {Hmac}
           */
          function makeHmac() { return null; }
          /**
           * @returns {{ id: string, refresh(): void, toJSON: () => object }}
           */
          function bundle() {
            return { id: 'x', refresh() {}, toJSON() { return {}; } };
          }
          function bodyMixed() {
            return { flag: true, run() { return 1; } };
          }
          module.exports = { makeHmac, bundle, bodyMixed };
        "#;
        let p = parse_js_exports(src, "matrix1.js");
        assert_eq!(
            p.tree.children.get("makeHmac").and_then(|n| n.member.as_ref()).and_then(|m| m.returns.as_deref()),
            Some("Hmac")
        );
        let br = p.tree.children.get("bundle").and_then(|n| n.member.as_ref()).and_then(|m| m.returns.clone()).expect("bundle");
        assert!(br.starts_with("$ret_"));
        let be = p.inline_types.get(&br).unwrap().child_entries_at("");
        let bm: HashMap<_, _> = be.iter().map(|m| (m.name.as_str(), m.is_method)).collect();
        assert_eq!(bm.get("id"), Some(&false));
        assert_eq!(bm.get("refresh"), Some(&true));
        assert_eq!(bm.get("toJSON"), Some(&true));
        let mr = p.tree.children.get("bodyMixed").and_then(|n| n.member.as_ref()).and_then(|m| m.returns.clone()).expect("bodyMixed");
        let me = p.inline_types.get(&mr).unwrap().child_entries_at("");
        let mm: HashMap<_, _> = me.iter().map(|m| (m.name.as_str(), m.is_method)).collect();
        assert_eq!(mm.get("flag"), Some(&false));
        assert_eq!(mm.get("run"), Some(&true));

        // --- 2) Method shorthand inside module.exports = { … } ---
        let src2 = r#"
          module.exports = {
            /**
             * @returns {Buffer}
             */
            raw() {},
            /**
             * @returns {{ token: string, renew(): void }}
             */
            issue() {},
            bodyOnly() {
              return { ok: true, stop() {} };
            },
            // data prop + method side by side on the export object itself
            version: '1',
            ping() {},
          };
        "#;
        let p2 = parse_js_exports(src2, "matrix2.js");
        assert_eq!(
            p2.tree.children.get("raw").and_then(|n| n.member.as_ref()).and_then(|m| m.returns.as_deref()),
            Some("Buffer")
        );
        let ir = p2.tree.children.get("issue").and_then(|n| n.member.as_ref()).and_then(|m| m.returns.clone()).expect("issue");
        let ie = p2.inline_types.get(&ir).unwrap().child_entries_at("");
        let im: HashMap<_, _> = ie.iter().map(|m| (m.name.as_str(), m.is_method)).collect();
        assert_eq!(im.get("token"), Some(&false));
        assert_eq!(im.get("renew"), Some(&true));
        let bor = p2.tree.children.get("bodyOnly").and_then(|n| n.member.as_ref()).and_then(|m| m.returns.clone()).expect("bodyOnly");
        let boe = p2.inline_types.get(&bor).unwrap().child_entries_at("");
        let bom: HashMap<_, _> = boe.iter().map(|m| (m.name.as_str(), m.is_method)).collect();
        assert_eq!(bom.get("ok"), Some(&false));
        assert_eq!(bom.get("stop"), Some(&true));
        // export object itself: data vs method
        assert_eq!(
            p2.tree.children.get("version").and_then(|n| n.member.as_ref()).map(|m| m.is_method),
            Some(false)
        );
        assert_eq!(
            p2.tree.children.get("ping").and_then(|n| n.member.as_ref()).map(|m| m.is_method),
            Some(true)
        );

        // --- 3) exports.x = / module.exports.x =  (single + mixed, JSDoc + body) ---
        let src3 = r#"
          /**
           * @returns {Hmac}
           */
          exports.asHmac = function () {};
          /**
           * @returns {{ left: string, right(): number }}
           */
          module.exports.pair = function () {};
          exports.fromBody = function () {
            return { a: 1, b() {} };
          };
          module.exports.arrowMixed = () => ({
            flag: false,
            go() { return 0; },
          });
        "#;
        let p3 = parse_js_exports(src3, "matrix3.js");
        assert_eq!(
            p3.tree.children.get("asHmac").and_then(|n| n.member.as_ref()).and_then(|m| m.returns.as_deref()),
            Some("Hmac")
        );
        let pr = p3.tree.children.get("pair").and_then(|n| n.member.as_ref()).and_then(|m| m.returns.clone()).expect("pair");
        let pe = p3.inline_types.get(&pr).unwrap().child_entries_at("");
        let pm: HashMap<_, _> = pe.iter().map(|m| (m.name.as_str(), m.is_method)).collect();
        assert_eq!(pm.get("left"), Some(&false));
        assert_eq!(pm.get("right"), Some(&true));
        let fr = p3.tree.children.get("fromBody").and_then(|n| n.member.as_ref()).and_then(|m| m.returns.clone()).expect("fromBody");
        let fe = p3.inline_types.get(&fr).unwrap().child_entries_at("");
        let fm: HashMap<_, _> = fe.iter().map(|m| (m.name.as_str(), m.is_method)).collect();
        assert_eq!(fm.get("a"), Some(&false));
        assert_eq!(fm.get("b"), Some(&true));
        let ar = p3.tree.children.get("arrowMixed").and_then(|n| n.member.as_ref()).and_then(|m| m.returns.clone()).expect("arrowMixed");
        let ae = p3.inline_types.get(&ar).unwrap().child_entries_at("");
        let am: HashMap<_, _> = ae.iter().map(|m| (m.name.as_str(), m.is_method)).collect();
        assert_eq!(am.get("flag"), Some(&false));
        assert_eq!(am.get("go"), Some(&true));

        // --- 4) End-to-end: require + call result members (single chain + mixed) ---
        let mut cat = load_builtin_catalog();
        let dir = std::env::temp_dir().join(format!("httpyac-matrix-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("kit.js"),
            r#"
              module.exports = {
                /** @returns {Hmac} */
                hmac() {},
                /** @returns {{ id: string, refresh(): void }} */
                session() { return { id: '1', refresh() {} }; },
              };
            "#,
        )
        .unwrap();
        let text = r#"
          const { hmac, session } = require('./kit.js');
          const h = hmac();
          const s = session();
        "#;
        let binds = enrich_catalog_from_requires(&mut cat, &dir, text);
        let vars = parse_typed_bindings(text, &mut cat, &binds);
        assert_eq!(vars.get("h").map(|s| s.as_str()), Some("Hmac"));
        let hmac_mem = cat.members_for_type("Hmac", "");
        assert!(
            hmac_mem.iter().any(|m| m.name == "update" || m.name == "digest"),
            "single type Hmac should expose chain methods"
        );
        let sty = vars.get("s").expect("session typed");
        let smem = cat.members_for_type(sty, "");
        let snames: HashMap<_, _> = smem.iter().map(|m| (m.name.as_str(), m.is_method)).collect();
        assert_eq!(snames.get("id"), Some(&false));
        assert_eq!(snames.get("refresh"), Some(&true));
        let _ = fs::remove_dir_all(&dir);
    }

    /// Mixed data properties + methods on one return shape.
    #[test]
    fn mixed_props_and_methods_on_return_shape() {
        let src = r#"
          /**
           * @returns {{
           *   id: string,
           *   count: number,
           *   refresh(): void,
           *   toJSON: () => object
           * }}
           */
          function bundle() {
            return {
              id: 'x',
              count: 1,
              refresh() {},
              toJSON() { return {}; },
            };
          }
          module.exports = { bundle };
        "#;
        let parsed = parse_js_exports(src, "mix.js");
        let ret = parsed
            .tree
            .children
            .get("bundle")
            .and_then(|n| n.member.as_ref())
            .and_then(|m| m.returns.clone())
            .expect("bundle returns");
        assert!(ret.starts_with("$ret_"), "{ret}");
        let tree = parsed.inline_types.get(&ret).expect("inline type");
        let entries = tree.child_entries_at("");
        let by: HashMap<_, _> = entries.iter().map(|m| (m.name.as_str(), m)).collect();
        assert!(by.contains_key("id"));
        assert!(!by["id"].is_method);
        assert!(by.contains_key("count"));
        assert!(by.contains_key("refresh"));
        assert!(by["refresh"].is_method, "refresh should be method");
        assert!(by.contains_key("toJSON"));
        assert!(by["toJSON"].is_method, "toJSON should be method");

        // body-only mixed (no JSDoc on second fn)
        let src2 = r#"
          function make() {
            return {
              flag: true,
              run() { return 1; },
              stop() {},
            };
          }
          module.exports = { make };
        "#;
        let p2 = parse_js_exports(src2, "mix2.js");
        let r2 = p2
            .tree
            .children
            .get("make")
            .and_then(|n| n.member.as_ref())
            .and_then(|m| m.returns.clone())
            .expect("make returns");
        let e2 = p2.inline_types.get(&r2).expect("make shape").child_entries_at("");
        let b2: HashMap<_, _> = e2.iter().map(|m| (m.name.as_str(), m.is_method)).collect();
        assert_eq!(b2.get("flag"), Some(&false));
        assert_eq!(b2.get("run"), Some(&true));
        assert_eq!(b2.get("stop"), Some(&true));
    }

    /// Phenomenon: any fn + any var names; RHS shape ⇒ `var.` members.
    #[test]
    fn phenomenon_any_names_call_result_members() {
        let mut cat = load_builtin_catalog();
        // Deliberately avoid demo names (signRequest/authDate).
        let mod_src = r#"
          /**
           * @returns {{ left: string, right: number }}
           */
          function weave(a, b) {
            return { left: a, right: b };
          }
          module.exports = { weave };
        "#;
        let dir = std::env::temp_dir().join(format!("httpyac-phenom-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("weave-util.js"), mod_src).unwrap();
        let text = r#"
          const { weave } = require('./weave-util.js');
          const bundle = weave('x', 2);
        "#;
        let binds = enrich_catalog_from_requires(&mut cat, &dir, text);
        let vars = parse_typed_bindings(text, &mut cat, &binds);
        let ty = vars.get("bundle").expect("bundle typed");
        let mem = cat.members_for_type(ty, "");
        let names: Vec<_> = mem.iter().map(|m| m.name.as_str()).collect();
        assert!(names.contains(&"left"), "{names:?}");
        assert!(names.contains(&"right"), "{names:?}");
        // destructure shaped result
        let text2 = r#"
          const { weave } = require('./weave-util.js');
          const { left, right } = weave('x', 2);
        "#;
        let binds2 = enrich_catalog_from_requires(&mut cat, &dir, text2);
        let vars2 = parse_typed_bindings(text2, &mut cat, &binds2);
        assert!(vars2.contains_key("left"));
        assert!(vars2.contains_key("right"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn object_literal_and_local_fn_and_multiline() {
        let mut cat = load_builtin_catalog();
        let text = r#"
          function pack(x) {
            return { alpha: 1, beta: 2 };
          }
          const packed = pack(1);
          const cfg = { host: 'h', port: 80 };
          const multi = pack(
            1
          );
        "#;
        ingest_script_locals(&mut cat, text);
        let binds = vec![];
        let vars = parse_typed_bindings(text, &mut cat, &binds);
        assert_eq!(vars.get("packed").map(|s| s.as_str()), Some("$ret_pack"));
        let mem = cat.members_for_type("$ret_pack", "");
        assert!(mem.iter().any(|m| m.name == "alpha"));
        assert!(mem.iter().any(|m| m.name == "beta"));
        assert!(vars.get("cfg").is_some_and(|t| t.starts_with("$obj_")));
        let cfg = cat.members_for_type(vars.get("cfg").unwrap(), "");
        assert!(cfg.iter().any(|m| m.name == "host"));
        assert!(cfg.iter().any(|m| m.name == "port"));
        assert_eq!(vars.get("multi").map(|s| s.as_str()), Some("$ret_pack"));
    }

    #[test]
    fn promise_unwrap_and_body_return_without_jsdoc() {
        let src = r#"
          /**
           * @returns {Promise<{ token: string, exp: number }>}
           */
          function issue() { return Promise.resolve({ token: 'a', exp: 1 }); }
          function bare() {
            return { ok: true, value: 3 };
          }
          module.exports = { issue, bare };
        "#;
        let parsed = parse_js_exports(src, "tok.js");
        assert!(
            parsed.inline_types.contains_key("$ret_issue")
                || parsed
                    .tree
                    .children
                    .get("issue")
                    .and_then(|n| n.member.as_ref())
                    .and_then(|m| m.returns.as_ref())
                    .is_some_and(|r| r.contains("token") || r.starts_with("$ret_")),
            "issue returns not materialized"
        );
        let bare_ret = parsed
            .tree
            .children
            .get("bare")
            .and_then(|n| n.member.as_ref())
            .and_then(|m| m.returns.clone());
        assert!(
            bare_ret.as_ref().is_some_and(|r| r.starts_with("$ret_bare") || r.contains("ok")),
            "bare body return missing: {bare_ret:?}"
        );
    }

    #[test]
    fn signed_return_object_members_from_jsdoc() {
        let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let examples = manifest.join("../examples");
        let http_dir = examples.canonicalize().unwrap_or(examples);
        let mut cat = load_builtin_catalog();
        let text = r#"
          const { signRequest } = require('./scripts/auth-sign.js');
          const signed = signRequest(request, 'secret');
        "#;
        let binds = enrich_catalog_from_requires(&mut cat, &http_dir, text);
        assert!(binds.iter().any(|b| b.name == "signRequest" && !b.maps_to_module));
        // signRequest should return $ret_signRequest with authDate / authentication
        let ret = cat.returns_of_path("auth-sign.signRequest");
        assert!(
            ret.as_deref() == Some("$ret_signRequest")
                || ret.as_ref().is_some_and(|t| t.contains("authDate") || t.starts_with("$ret_")),
            "unexpected returns: {ret:?}"
        );
        let vars = parse_typed_bindings(text, &mut cat, &binds);
        assert_eq!(
            vars.get("signed").map(|s| s.as_str()),
            Some("$ret_signRequest"),
            "signed should be inferred from signRequest @returns; got {vars:?}"
        );
        let mem = cat.members_for_type("$ret_signRequest", "");
        let names: Vec<_> = mem.iter().map(|m| m.name.as_str()).collect();
        assert!(names.contains(&"authDate"), "got {names:?}");
        assert!(names.contains(&"authentication"), "got {names:?}");
    }


    /// Typing `signRe` after `const { signRequest } = require(...)` must offer signRequest.
    #[test]
    fn bare_ident_offers_destructured_require() {
        let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let examples = manifest.join("../examples");
        let http_dir = examples.canonicalize().unwrap_or(examples);
        let mut cat = load_builtin_catalog();
        // Same shape as script-vtsls.http island (cursor on "signRe")
        let text = r#"
{{
  const { signRequest } = require('./scripts/auth-sign.js');
  const signed = signRequest(request, 'secret');
  exports.authDate = signed.authDate;
  signRe
}}
"#;
        let binds = enrich_catalog_from_requires(&mut cat, &http_dir, text);
        assert!(
            binds.iter().any(|b| b.name == "signRequest"),
            "require destructure not parsed: {binds:?}"
        );
        assert!(
            cat.modules.contains_key("auth-sign"),
            "auth-sign.js not loaded from require — path resolve failed under {http_dir:?}"
        );
        // Simulate bare filter
        let filter = "signre";
        let hits: Vec<_> = binds
            .iter()
            .filter(|b| b.name.to_lowercase().starts_with(filter))
            .map(|b| b.name.as_str())
            .collect();
        assert!(
            hits.contains(&"signRequest"),
            "filter signRe should hit signRequest, got {hits:?} from {binds:?}"
        );
        // signed. members
        let vars = parse_typed_bindings(text, &mut cat, &binds);
        assert!(vars.contains_key("signed"), "signed not typed: {vars:?}");
        let ty = vars.get("signed").unwrap();
        let mem = cat.members_for_type(ty, "");
        let names: Vec<_> = mem.iter().map(|m| m.name.as_str()).collect();
        assert!(names.contains(&"authDate"), "signed. missing authDate: {names:?} ty={ty}");
    }

    #[test]
    fn enrich_loads_relative_require_js() {
        let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let examples = manifest.join("../examples");
        let http_dir = examples.canonicalize().unwrap_or(examples);
        let mut cat = load_builtin_catalog();
        let text = r#"
          const helpers = require('./scripts/auth-sign.js');
          const { signRequest } = require("./scripts/auth-sign.js");
        "#;
        let binds = enrich_catalog_from_requires(&mut cat, &http_dir, text);
        assert!(
            cat.modules.contains_key("auth-sign"),
            "auth-sign module should be loaded from require path"
        );
        let mem = cat.members_for_path("auth-sign");
        assert!(
            mem.iter().any(|m| m.name == "signRequest"),
            "signRequest export missing: {:?}",
            mem.iter().map(|m| &m.name).collect::<Vec<_>>()
        );
        assert!(
            mem.iter().any(|m| m.name == "hashHex"),
            "hashHex export missing"
        );
        // whole-module binding
        assert!(binds.iter().any(|b| b.name == "helpers" && b.maps_to_module));
        assert_eq!(
            resolve_path_with_bindings("helpers", &binds),
            "auth-sign"
        );
        // destructure is bare-only
        assert!(binds
            .iter()
            .any(|b| b.name == "signRequest" && !b.maps_to_module));
    }

    #[test]
    fn parse_nested_module_exports() {
        let src = r#"
            module.exports = {
              sign(data) { return data; },
              utils: {
                hex: (s) => s,
                nest: {
                  deep() { return 1; }
                }
              },
              VERSION: '1'
            };
        "#;
        let parsed = parse_js_exports(src, "crypto.js");
        let top = parsed.tree.child_entries_at("");
        let names: Vec<_> = top.iter().map(|m| m.name.as_str()).collect();
        assert!(names.contains(&"sign"));
        assert!(names.contains(&"utils"));
        assert!(names.contains(&"VERSION"));

        let utils = parsed.tree.child_entries_at("utils");
        let unames: Vec<_> = utils.iter().map(|m| m.name.as_str()).collect();
        assert!(unames.contains(&"hex"));
        assert!(unames.contains(&"nest"));

        let deep = parsed.tree.child_entries_at("utils.nest");
        assert!(deep.iter().any(|m| m.name == "deep"));
    }

    #[test]
    fn parse_functions_and_exports_dot() {
        let src = r#"
            function hashHex(data) { return data; }
            exports.signRequest = function(req) {};
            module.exports.extra = () => {};
        "#;
        let parsed = parse_js_exports(src, "auth.js");
        let names: Vec<_> = parsed
            .tree
            .child_entries_at("")
            .iter()
            .map(|m| m.name.clone())
            .collect();
        assert!(names.iter().any(|n| n == "hashHex"));
        assert!(names.iter().any(|n| n == "signRequest"));
        assert!(names.iter().any(|n| n == "extra"));
    }

    #[test]
    fn httpyac_path_tag() {
        let src = r#"
            /**
             * @httpyac-path request.headers
             */
            module.exports = {
              set(name, value) {},
              get(name) {},
            };
        "#;
        let parsed = parse_js_exports(src, "headers.js");
        assert!(parsed.path_attachments.contains_key("request.headers"));
        let mem = parsed.path_attachments["request.headers"].child_entries_at("");
        assert!(mem.iter().any(|m| m.name == "set"));
        assert!(mem.iter().any(|m| m.name == "get"));
    }

    #[test]
    fn load_catalog_from_dir() {
        let dir = std::env::temp_dir().join(format!(
            "httpyac-script-test-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let mut f = fs::File::create(dir.join("crypto.js")).unwrap();
        writeln!(
            f,
            r#"
            module.exports = {{
              createHmac() {{}},
              utils: {{ hex() {{}} }}
            }};
            "#
        )
        .unwrap();
        let mut f2 = fs::File::create(dir.join("request.headers.js")).unwrap();
        writeln!(
            f2,
            r#"module.exports = {{ set() {{}}, get() {{}} }};"#
        )
        .unwrap();

        let cat = load_script_catalog(&dir);
        assert!(cat.modules.contains_key("crypto"));
        let c = cat.members_for_path("crypto");
        assert!(c.iter().any(|m| m.name == "createHmac"));
        assert!(c.iter().any(|m| m.name == "utils"));
        let u = cat.members_for_path("crypto.utils");
        assert!(u.iter().any(|m| m.name == "hex"));

        let h = cat.members_for_path("request.headers");
        assert!(h.iter().any(|m| m.name == "set"));

        let _ = fs::remove_dir_all(&dir);
    }
}

#[cfg(test)]
mod builtin_and_override_tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn builtin_has_request_response_client_console() {
        let cat = load_builtin_catalog();
        for name in ["request", "response", "client", "console", "crypto"] {
            assert!(cat.modules.contains_key(name), "missing builtin {name}");
        }
        let req = cat.members_for_path("request");
        assert!(req.iter().any(|m| m.name == "url"));
        assert!(req.iter().any(|m| m.name == "headers"));
        let rh = cat.members_for_path("response.headers");
        assert!(rh.iter().any(|m| m.name == "get") || rh.iter().any(|m| m.name == "valueOf"));
        let cg = cat.members_for_path("client.global");
        assert!(cg.iter().any(|m| m.name == "set"));
        let cry = cat.members_for_path("crypto");
        assert!(cry.iter().any(|m| m.name == "createHmac"));
    }

    #[test]
    fn user_overrides_builtin_member() {
        let dir = std::env::temp_dir().join(format!(
            "httpyac-override-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let mut f = fs::File::create(dir.join("request.js")).unwrap();
        writeln!(
            f,
            r#"module.exports = {{ customFlag() {{}}, url: 'overridden-doc' }};"#
        )
        .unwrap();

        let cat = build_catalog(Some(&dir));
        let req = cat.members_for_path("request");
        assert!(
            req.iter().any(|m| m.name == "customFlag"),
            "user-added member missing"
        );
        // builtin members still present unless fully replaced key-by-key
        assert!(req.iter().any(|m| m.name == "method") || req.iter().any(|m| m.name == "headers"));
        // url overridden (user leaf wins)
        let url = req.iter().find(|m| m.name == "url");
        assert!(url.is_some());

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn cache_always_returns_builtin_without_user_dir() {
        let mut cache = ScriptCatalogCache::default();
        let cat = cache.get_or_load(Path::new("/tmp/no-such-httpyac-project-xyz"));
        assert!(cat.modules.contains_key("request"));
        assert!(cat.modules.contains_key("crypto"));
    }

    #[test]
    fn parse_require_in_environments_snippet() {
        let text = r#"
{{
  const crypto = require('crypto');
  cryp
}}
"#;
        let b = parse_require_bindings(text);
        assert!(b.iter().any(|x| x.name == "crypto"));
        assert_eq!(resolve_path_with_bindings("crypto", &b), "crypto");
    }

    #[test]
    fn returns_annotation_and_hmac_chain() {
        let cat = load_builtin_catalog();
        assert!(cat.types.contains_key("Hmac"), "Hmac type missing");
        let cry = cat.members_for_path("crypto");
        let create = cry.iter().find(|m| m.name == "createHmac");
        assert!(create.is_some());
        assert_eq!(
            create.and_then(|m| m.returns.as_deref()),
            Some("Hmac"),
            "createHmac should @returns Hmac"
        );
        assert_eq!(
            cat.returns_of_path("crypto.createHmac").as_deref(),
            Some("Hmac")
        );
        let hmac_mem = cat.members_for_type("Hmac", "");
        assert!(hmac_mem.iter().any(|m| m.name == "update"));
        assert!(hmac_mem.iter().any(|m| m.name == "digest"));
    }

    #[test]
    fn infer_const_hmac_binding() {
        let mut cat = load_builtin_catalog();
        let text = r#"
          const crypto = require('crypto');
          const h = crypto.createHmac('sha256', 'secret');
          const h2 = h.update('data');
        "#;
        let req = parse_require_bindings(text);
        let vars = parse_typed_bindings(text, &mut cat, &req);
        assert_eq!(vars.get("h").map(|s| s.as_str()), Some("Hmac"));
        assert_eq!(vars.get("h2").map(|s| s.as_str()), Some("Hmac"));
        match resolve_completion_path("h", &cat, &req, &vars) {
            PathResolve::Type { type_name, rest } => {
                assert_eq!(type_name, "Hmac");
                assert!(rest.is_empty());
            }
            other => panic!("expected Type, got {other:?}"),
        }
    }

    #[test]
    fn user_crypto_override_keeps_createhmac_returns() {
        let dir = std::env::temp_dir().join(format!(
            "httpyac-crypto-override-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        // User overrides createHmac WITHOUT @returns — must still chain via preserve/fallback
        fs::write(
            dir.join("crypto.js"),
            r#"module.exports = { createHmac(a,k) { return {}; }, sign() {} };"#,
        )
        .unwrap();
        let mut cat = build_catalog(Some(&dir));
        assert_eq!(
            cat.returns_of_path("crypto.createHmac").as_deref(),
            Some("Hmac")
        );
        let text = "const crypto = require('crypto');\nconst sha256 = crypto.createHmac('sha256', 'secret');\n";
        let req = parse_require_bindings(text);
        let vars = parse_typed_bindings(text, &mut cat, &req);
        assert_eq!(vars.get("sha256").map(|s| s.as_str()), Some("Hmac"));
        let mem = cat.members_for_type("Hmac", "");
        assert!(mem.iter().any(|m| m.name == "update"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn hostile_script_does_not_abort_catalog() {
        let dir = std::env::temp_dir().join(format!(
            "httpyac-hostile-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        // Unbalanced / nonsense — parser must not unwind the process
        fs::write(dir.join("broken.js"), "module.exports = { a: ((((").unwrap();
        fs::write(
            dir.join("ok.js"),
            "module.exports = { hello() {} };",
        )
        .unwrap();
        let cat = build_catalog(Some(&dir));
        assert!(
            cat.modules.contains_key("request"),
            "builtins must still load"
        );
        assert!(
            cat.modules.contains_key("ok") || cat.modules.contains_key("crypto"),
            "catalog still usable after hostile file"
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn oversized_script_file_skipped() {
        let dir = std::env::temp_dir().join(format!(
            "httpyac-oversize-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let big = "x".repeat((MAX_USER_SCRIPT_FILE_BYTES as usize) + 8);
        fs::write(
            dir.join("huge.js"),
            format!("module.exports = {{ huge: '{big}' }};"),
        )
        .unwrap();
        fs::write(dir.join("small.js"), "module.exports = { tiny() {} };").unwrap();
        let user = load_script_catalog(&dir);
        assert!(
            !user.modules.contains_key("huge"),
            "oversized file must be skipped"
        );
        assert!(user.modules.contains_key("small"));
        let _ = fs::remove_dir_all(&dir);
    }
}


    #[test]
    fn script_window_includes_require_when_cursor_on_later_line() {
        let src = r#"###
{{
  const { signRequest } = require('./scripts/auth-sign.js');
  const signed = signRequest(request, 'secret');
  signRe
}}
POST https://x
"#;
        let lines: Vec<&str> = src.lines().collect();
        // line index of signRe
        let idx = lines.iter().position(|l| l.contains("signRe")).unwrap();
        let window = script_window_text(&lines, idx);
        eprintln!("window=\n{window}");
        assert!(window.contains("require('./scripts/auth-sign.js')"), "window missing require: {window}");
        assert!(window.contains("signRequest"), "window missing signRequest binding line");
    }
