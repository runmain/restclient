//! Shared library for httpyac-lsp and httpyac-run.
//!
//! Request **execution** is always delegated to the native `httpyac` CLI.
//! This crate only helps discover env names and build CLI arguments.

pub mod completions;
pub mod parser;
pub mod script_ext;
pub mod variables;

use std::path::{Path, PathBuf};
use std::process::Command;

use variables::VariableResolver;

/// Resolve absolute path to httpyac CLI.
pub fn resolve_httpyac_bin() -> String {
    if let Ok(p) = std::env::var("HTTPYAC_BIN") {
        let p = p.trim();
        if !p.is_empty() {
            return p.to_string();
        }
    }

    let which_cmd = if cfg!(windows) { "where" } else { "which" };
    if let Ok(output) = Command::new(which_cmd).arg("httpyac").output() {
        if output.status.success() {
            let path = String::from_utf8_lossy(&output.stdout)
                .lines()
                .next()
                .unwrap_or("")
                .trim()
                .to_string();
            if !path.is_empty() && Path::new(&path).exists() {
                return path;
            }
        }
    }

    let home = std::env::var("HOME").unwrap_or_default();
    let mut candidates: Vec<String> = vec![
        format!("{home}/.local/bin/httpyac"),
        "/usr/local/bin/httpyac".to_string(),
        "/opt/homebrew/bin/httpyac".to_string(),
        "/usr/bin/httpyac".to_string(),
    ];
    let nvm_dir = format!("{home}/.nvm/versions/node");
    if let Ok(entries) = std::fs::read_dir(&nvm_dir) {
        let mut versions: Vec<_> = entries.flatten().map(|e| e.path()).collect();
        versions.sort();
        versions.reverse();
        for ver in versions {
            candidates.push(ver.join("bin/httpyac").to_string_lossy().to_string());
        }
    }
    for c in candidates {
        if Path::new(&c).is_file() {
            return c;
        }
    }
    "httpyac".to_string()
}

/// Env names + optional activeEnv from http-client.env.json walking parents.
pub fn collect_env_names(file_dir: &Path) -> (Option<String>, Vec<String>) {
    let mut resolver = VariableResolver::new();
    if !resolver.load_environments_from_dir(file_dir) {
        return (None, Vec::new());
    }
    let mut names = resolver.get_available_environment_names();
    names.sort();
    (resolver.get_default_environment_name(), names)
}

/// Effective env for Send = `activeEnv` in nearest http-client.env.json only.
pub fn resolve_effective_env(file_dir: &Path) -> (Option<String>, Vec<String>) {
    collect_env_names(file_dir)
}

/// Persist selected env by updating `activeEnv` in the nearest `http-client.env.json`.
/// Does **not** create any extra files.
pub fn save_active_env(file_dir: &Path, env_name: &str) -> Result<(), String> {
    let json_path = find_nearest_env_json(file_dir).ok_or_else(|| {
        format!(
            "未找到 http-client.env.json（从 {} 向上搜索）。无法保存 activeEnv。",
            file_dir.display()
        )
    })?;
    update_active_env_in_json(&json_path, env_name)?;
    Ok(())
}

fn find_nearest_env_json(start_dir: &Path) -> Option<PathBuf> {
    let mut dir = start_dir.to_path_buf();
    loop {
        let p = dir.join(variables::ENV_FILE_PUBLIC);
        if p.is_file() {
            return Some(p);
        }
        if !dir.pop() {
            return None;
        }
    }
}

fn update_active_env_in_json(path: &Path, env_name: &str) -> Result<(), String> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| format!("读取 {}: {e}", path.display()))?;
    let mut value: serde_json::Value = serde_json::from_str(&content)
        .map_err(|e| format!("解析 {}: {e}", path.display()))?;
    let obj = value
        .as_object_mut()
        .ok_or_else(|| format!("{} 顶层不是对象", path.display()))?;
    obj.insert(
        "activeEnv".to_string(),
        serde_json::Value::String(env_name.to_string()),
    );
    let pretty = serde_json::to_string_pretty(&value)
        .map_err(|e| format!("序列化失败: {e}"))?;
    std::fs::write(path, format!("{pretty}\n"))
        .map_err(|e| format!("写入 {}: {e}", path.display()))?;
    eprintln!("  已更新: {} → activeEnv = \"{env_name}\"", path.display());
    Ok(())
}

/// Parent directory of an .http file.
pub fn file_dir(file: &Path) -> PathBuf {
    file.parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."))
}

/// Build `httpyac send` args: send <file> --line <n> [--env <name>]
pub fn build_httpyac_send_args(file: &str, line: &str, env: Option<&str>) -> Vec<String> {
    let mut args = vec![
        "send".to_string(),
        file.to_string(),
        "--line".to_string(),
        line.to_string(),
    ];
    if let Some(e) = env {
        if !e.is_empty() {
            args.push("--env".to_string());
            args.push(e.to_string());
        }
    }
    args
}

/// Ensure request URL has a scheme before send.
///
/// Rules (always prefer **http://**, never https — hosts may be bare IPs):
/// - Already has `://` → unchanged
/// - Path-only (`/api/...`) + `Host:` header → `http://{host}{path}`
/// - Otherwise (host, host/path, IP, IP:port/path) → prefix `http://`
///
/// `host_header` should be the raw Host header value (no `Host:` prefix).
pub fn ensure_http_url(url: &str, host_header: Option<&str>) -> (String, bool) {
    let u = url.trim();
    if u.is_empty() {
        return (u.to_string(), false);
    }
    // Never rewrite URLs that still contain unresolved {{variables}}
    // e.g. {{baseUrl}}/get must NOT become http://{{baseUrl}}/get
    if u.contains("{{") {
        return (u.to_string(), false);
    }
    // Already absolute with scheme (http, https, ws, …)
    if let Some(idx) = u.find("://") {
        if idx > 0
            && u[..idx]
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '-' || c == '.')
        {
            return (u.to_string(), false);
        }
    }

    let host = host_header.map(|h| {
        let h = h.trim();
        // Strip accidental scheme on Host value
        if let Some(rest) = h.split_once("://") {
            rest.1.trim().to_string()
        } else {
            h.to_string()
        }
    });

    if u.starts_with('/') {
        if let Some(h) = host {
            if !h.is_empty() {
                return (format!("http://{h}{u}"), true);
            }
        }
        // Path without Host — still force http scheme on localhost is wrong;
        // leave path as-is for httpyac to error, or use http:// + path (invalid).
        // Prefer: if we have no host, just prepend http:// to nothing useful —
        // return unchanged so user sees clear error.
        return (u.to_string(), false);
    }

    // Bare host / IP / host:port / host/path
    (format!("http://{u}"), true)
}

/// Rewrite the request-line URL at 1-based `line` in file content if scheme is missing.
/// Returns (new_content, optional note about the rewrite).
pub fn rewrite_url_scheme_in_content(
    content: &str,
    line_1based: usize,
) -> Result<(String, Option<String>), String> {
    use parser::parse_http_file;

    let requests = parse_http_file(content).unwrap_or_default();
    let req = requests
        .iter()
        .find(|r| {
            line_1based >= r.metadata.block_start_line && line_1based <= r.metadata.end_line
        })
        .or_else(|| {
            requests
                .iter()
                .find(|r| r.metadata.start_line == line_1based)
        });

    let Some(req) = req else {
        return Ok((content.to_string(), None));
    };

    let host = req
        .headers
        .iter()
        .find(|(n, _)| n.eq_ignore_ascii_case("host"))
        .map(|(_, v)| v.as_str());

    // URL in parsed request may already have variables resolved; use raw line from file
    let lines: Vec<&str> = content.lines().collect();
    let req_line_idx = req.metadata.start_line.saturating_sub(1);
    if req_line_idx >= lines.len() {
        return Ok((content.to_string(), None));
    }
    let raw_line = lines[req_line_idx];
    let trimmed = raw_line.trim_start();
    let indent_len = raw_line.len() - trimmed.len();
    let indent = &raw_line[..indent_len];

    // METHOD URL [HTTP/x]
    let mut parts = trimmed.split_whitespace();
    let method = parts.next().unwrap_or("");
    let url = parts.next().unwrap_or("");
    let rest: Vec<&str> = parts.collect();
    if method.is_empty() || url.is_empty() {
        return Ok((content.to_string(), None));
    }

    let (new_url, changed) = ensure_http_url(url, host);
    if !changed {
        return Ok((content.to_string(), None));
    }

    let mut new_line = format!("{indent}{method} {new_url}");
    if !rest.is_empty() {
        new_line.push(' ');
        new_line.push_str(&rest.join(" "));
    }

    let mut out_lines: Vec<String> = content.lines().map(|s| s.to_string()).collect();
    // Preserve whether file ended with newline
    let ended_nl = content.ends_with('\n');
    out_lines[req_line_idx] = new_line;
    let mut out = out_lines.join("\n");
    if ended_nl {
        out.push('\n');
    }

    let note = match host {
        Some(h) if url.starts_with('/') => {
            format!("URL scheme: `{url}` + Host `{h}` → `{new_url}` (http, not https)")
        }
        _ => format!("URL scheme: `{url}` → `{new_url}` (default http:// for host/IP)"),
    };
    Ok((out, Some(note)))
}

#[cfg(test)]
mod url_scheme_tests {
    use super::ensure_http_url;

    #[test]
    fn keeps_existing_scheme() {
        let (u, c) = ensure_http_url("https://a.com/x", None);
        assert!(!c);
        assert_eq!(u, "https://a.com/x");
        let (u, c) = ensure_http_url("http://1.2.3.4/y", None);
        assert!(!c);
        assert_eq!(u, "http://1.2.3.4/y");
    }

    #[test]
    fn bare_host_gets_http() {
        let (u, c) = ensure_http_url("www.baidu.com", None);
        assert!(c);
        assert_eq!(u, "http://www.baidu.com");
    }

    #[test]
    fn bare_ip_gets_http_not_https() {
        let (u, c) = ensure_http_url("192.168.1.1:8080/api", None);
        assert!(c);
        assert_eq!(u, "http://192.168.1.1:8080/api");
    }

    #[test]
    fn path_with_host_header() {
        let (u, c) = ensure_http_url("/get", Some("www.baidu.com"));
        assert!(c);
        assert_eq!(u, "http://www.baidu.com/get");
    }

    #[test]
    fn path_without_host_unchanged() {
        let (u, c) = ensure_http_url("/get", None);
        assert!(!c);
        assert_eq!(u, "/get");
    }

    #[test]
    fn leaves_mustache_urls_alone() {
        let (u, c) = ensure_http_url("{{baseUrl}}/get", None);
        assert!(!c);
        assert_eq!(u, "{{baseUrl}}/get");
        let (u, c) = ensure_http_url("{{baseUrl}}/get", Some("example.com"));
        assert!(!c);
        assert_eq!(u, "{{baseUrl}}/get");
    }
}
