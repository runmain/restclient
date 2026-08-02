use std::path::PathBuf;

use zed_extension_api::{self as zed, settings::LspSettings, LanguageServerId, Result};

struct HttpyacClientExtension {}

impl HttpyacClientExtension {
    /// Resolve httpyac-lsp binary, in priority order:
    /// 1) user settings `lsp.httpyac-lsp.binary.path`
    /// 2) PATH (`worktree.which`)
    /// 3) extension-bundled `lsp/httpyac-lsp` (Zed sets cwd to the extension dir)
    /// 4) `$HOME/.local/bin/httpyac-lsp`
    fn resolve_lsp_binary(
        &self,
        language_server_id: &LanguageServerId,
        worktree: &zed::Worktree,
    ) -> Result<(String, Vec<String>, Vec<(String, String)>)> {
        let mut env: Vec<(String, String)> = worktree.shell_env();

        if let Some(httpyac) = worktree.which("httpyac") {
            env.push(("HTTPYAC_BIN".to_string(), httpyac));
        }

        // Child vtsls for script-region IntelliSense (httpyac-lsp spawns it).
        // Priority: lsp.httpyac-lsp.settings.vtslsCommand → PATH vtsls
        let mut vtsls_from_settings: Option<String> = None;
        if let Ok(settings) = LspSettings::for_worktree(language_server_id.as_ref(), worktree) {
            if let Some(s) = settings.settings.as_ref() {
                for key in ["vtslsCommand", "vtsls_command"] {
                    if let Some(c) = s.get(key).and_then(|v| v.as_str()) {
                        let c = c.trim();
                        if !c.is_empty() {
                            vtsls_from_settings = Some(c.to_string());
                            break;
                        }
                    }
                }
            }
        }
        if let Some(v) = vtsls_from_settings.or_else(|| worktree.which("vtsls")) {
            env.push(("HTTPYAC_VTSLS_COMMAND".to_string(), v));
        }

        // 1) settings override
        if let Ok(settings) = LspSettings::for_worktree(language_server_id.as_ref(), worktree) {
            if let Some(binary) = settings.binary {
                if let Some(path) = binary.path {
                    if !path.is_empty() {
                        if let Some(bin_env) = binary.env {
                            for (k, v) in bin_env {
                                env.push((k, v));
                            }
                        }
                        return Ok((path, binary.arguments.unwrap_or_default(), env));
                    }
                }
            }
        }

        // 2) PATH
        if let Some(path) = worktree.which("httpyac-lsp") {
            return Ok((path, vec![], env));
        }

        // 3) bundled next to extension.wasm (cwd = extension install dir)
        if let Ok(cwd) = std::env::current_dir() {
            for rel in ["lsp/httpyac-lsp", "httpyac-lsp"] {
                let candidate = cwd.join(rel);
                if candidate.is_file() {
                    return Ok((candidate.to_string_lossy().to_string(), vec![], env));
                }
            }
            // Also try absolute from known install layouts
            let _ = PathBuf::from(&cwd);
        }

        // 4) ~/.local/bin
        let home = env
            .iter()
            .find(|(k, _)| k == "HOME")
            .map(|(_, v)| v.clone())
            .or_else(|| {
                env.iter()
                    .find(|(k, _)| k == "USERPROFILE")
                    .map(|(_, v)| v.clone())
            })
            .unwrap_or_else(|| ".".to_string());

        let local = format!(
            "{}/.local/bin/httpyac-lsp",
            home.trim_end_matches(['/', '\\'])
        );
        if std::path::Path::new(&local).is_file() {
            return Ok((local, vec![], env));
        }

        Err(format!(
            "httpyac-lsp not found. Install with ./install_to_zed.sh \
             or set lsp.httpyac-lsp.binary.path in settings.json \
             (tried PATH, extension lsp/, {local})"
        ))
    }
}

impl zed::Extension for HttpyacClientExtension {
    fn new() -> Self {
        Self {}
    }

    fn language_server_command(
        &mut self,
        language_server_id: &LanguageServerId,
        worktree: &zed::Worktree,
    ) -> Result<zed::Command> {
        let (command, args, env) = self.resolve_lsp_binary(language_server_id, worktree)?;
        Ok(zed::Command {
            command,
            args,
            env,
        })
    }

    fn language_server_initialization_options(
        &mut self,
        language_server_id: &LanguageServerId,
        worktree: &zed::Worktree,
    ) -> Result<Option<serde_json::Value>> {
        let mut map = serde_json::Map::new();
        // Seed vtsls path for script-region proxy
        if let Ok(settings) = LspSettings::for_worktree(language_server_id.as_ref(), worktree) {
            if let Some(s) = settings.settings {
                if let Some(obj) = s.as_object() {
                    for (k, v) in obj {
                        map.insert(k.clone(), v.clone());
                    }
                }
            }
        }
        if !map.contains_key("vtslsCommand") && !map.contains_key("vtsls_command") {
            if let Some(v) = worktree.which("vtsls") {
                map.insert("vtslsCommand".into(), serde_json::Value::String(v));
            }
        }
        if !map.contains_key("vtslsEnabled") {
            map.insert("vtslsEnabled".into(), serde_json::Value::Bool(true));
        }
        if map.is_empty() {
            Ok(None)
        } else {
            Ok(Some(serde_json::Value::Object(map)))
        }
    }

    fn language_server_workspace_configuration(
        &mut self,
        language_server_id: &LanguageServerId,
        worktree: &zed::Worktree,
    ) -> Result<Option<serde_json::Value>> {
        // Forward optional `lsp.httpyac-lsp.settings` from user config
        Ok(LspSettings::for_worktree(language_server_id.as_ref(), worktree)
            .ok()
            .and_then(|s| s.settings))
    }
}

zed::register_extension!(HttpyacClientExtension);
