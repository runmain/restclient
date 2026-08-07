use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Environment {
    #[serde(flatten)]
    pub variables: HashMap<String, String>,
}

/// Standard JetBrains HTTP Client environment file names
pub const ENV_FILE_PUBLIC: &str = "http-client.env.json";
pub const ENV_FILE_PRIVATE: &str = "http-client.private.env.json";

pub struct VariableResolver {
    environments: HashMap<String, Environment>,
    active_environment: Option<String>,
    global_variables: HashMap<String, String>,
    file_variables: HashMap<String, String>,
}

impl Default for VariableResolver {
    fn default() -> Self {
        Self::new()
    }
}

#[allow(dead_code)] // Public API — some methods used only by tests or external consumers
impl VariableResolver {
    pub fn new() -> Self {
        Self {
            environments: HashMap::new(),
            active_environment: None,
            global_variables: HashMap::new(),
            file_variables: HashMap::new(),
        }
    }

    /// Load environments from a single JSON file.
    /// Expects `"activeEnv"` key to specify the active environment.
    pub fn load_environments<P: AsRef<Path>>(&mut self, path: P) -> Result<(), String> {
        let content = fs::read_to_string(path)
            .map_err(|e| format!("Failed to read environments file: {}", e))?;

        let value: serde_json::Value = serde_json::from_str(&content)
            .map_err(|e| format!("Failed to parse environments JSON: {}", e))?;

        let obj = value
            .as_object()
            .ok_or("Expected JSON object at top level")?;

        let active_env = obj
            .get("activeEnv")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        // Parse all keys except "activeEnv" as environments
        let mut envs = HashMap::new();
        for (key, val) in obj {
            if key == "activeEnv" {
                continue;
            }
            let env: Environment = serde_json::from_value(val.clone())
                .map_err(|e| format!("Failed to parse environment '{}': {}", key, e))?;
            envs.insert(key.clone(), env);
        }

        self.environments = envs;

        if let Some(name) = &active_env {
            if self.environments.contains_key(name) {
                self.active_environment = active_env;
            } else {
                return Err(format!(
                    "activeEnv '{}' not found. Available: {:?}",
                    name,
                    self.environments.keys().collect::<Vec<_>>()
                ));
            }
        }

        Ok(())
    }

    /// Load environments using JetBrains convention: http-client.env.json (public)
    /// merged with http-client.private.env.json (private overrides public).
    ///
    /// Walks **all** parent directories from `start_dir` upward (file dir → project root),
    /// merging every `http-client.env.json` / private file found. Closer directories win
    /// on key conflicts (loaded last).
    pub fn load_environments_from_dir(&mut self, start_dir: &Path) -> bool {
        self.load_environments_from_dir_with_errors(start_dir).0
    }

    /// Load environments and return non-fatal file errors for callers that
    /// need to surface diagnostics before delegating execution to httpyac.
    pub fn load_environments_from_dir_with_errors(
        &mut self,
        start_dir: &Path,
    ) -> (bool, Vec<String>) {
        let dirs = find_all_env_dirs_walking_up(start_dir);
        if dirs.is_empty() {
            return (false, Vec::new());
        }

        let mut errors = Vec::new();
        // Load from root → leaf so nearer files override
        for search_dir in dirs.iter().rev() {
            let public_file = search_dir.join(ENV_FILE_PUBLIC);
            if public_file.exists() {
                if let Err(e) = self.merge_environments_file(&public_file) {
                    errors.push(format!("{}: {e}", public_file.display()));
                }
            }

            let private_file = search_dir.join(ENV_FILE_PRIVATE);
            if private_file.exists() {
                match Self::read_environments(&private_file) {
                    Ok(private_envs) => {
                        for (env_name, private_env) in private_envs {
                            let entry =
                                self.environments
                                    .entry(env_name)
                                    .or_insert_with(|| Environment {
                                        variables: HashMap::new(),
                                    });
                            for (key, value) in private_env.variables {
                                entry.variables.insert(key, value);
                            }
                        }
                    }
                    Err(e) => errors.push(format!("{}: {e}", private_file.display())),
                }
            }
        }

        (!self.environments.is_empty(), errors)
    }

    /// Merge env definitions from a JSON file without wiping existing envs.
    fn merge_environments_file(&mut self, path: &Path) -> Result<(), String> {
        let content = fs::read_to_string(path)
            .map_err(|e| format!("Failed to read environments file: {}", e))?;
        let value: serde_json::Value = serde_json::from_str(&content)
            .map_err(|e| format!("Failed to parse environments JSON: {}", e))?;
        let obj = value
            .as_object()
            .ok_or("Expected JSON object at top level")?;

        if let Some(name) = obj.get("activeEnv").and_then(|v| v.as_str()) {
            // Prefer activeEnv from nearer files (this call is leaf-last)
            self.active_environment = Some(name.to_string());
        }

        for (key, val) in obj {
            if key == "activeEnv" {
                continue;
            }
            let env: Environment = serde_json::from_value(val.clone())
                .map_err(|e| format!("Failed to parse environment '{}': {}", key, e))?;
            let entry = self
                .environments
                .entry(key.clone())
                .or_insert_with(|| Environment {
                    variables: HashMap::new(),
                });
            for (k, v) in env.variables {
                entry.variables.insert(k, v);
            }
        }
        Ok(())
    }

    fn read_environments(path: &Path) -> Result<HashMap<String, Environment>, String> {
        let content = fs::read_to_string(path)
            .map_err(|e| format!("Failed to read {}: {}", path.display(), e))?;
        let value: serde_json::Value = serde_json::from_str(&content)
            .map_err(|e| format!("Failed to parse {}: {}", path.display(), e))?;
        let obj = value
            .as_object()
            .ok_or_else(|| format!("Expected JSON object in {}", path.display()))?;
        let mut envs = HashMap::new();
        for (key, val) in obj {
            if key == "activeEnv" {
                continue;
            }
            let env: Environment = serde_json::from_value(val.clone())
                .map_err(|e| format!("Failed to parse environment '{}': {}", key, e))?;
            envs.insert(key.clone(), env);
        }
        Ok(envs)
    }

    /// Set the active environment
    pub fn set_active_environment(&mut self, name: String) -> Result<(), String> {
        if self.environments.contains_key(&name) {
            self.active_environment = Some(name);
            Ok(())
        } else {
            Err(format!("Environment '{}' not found", name))
        }
    }

    /// Get the active environment name
    pub fn get_active_environment(&self) -> Option<&String> {
        self.active_environment.as_ref()
    }

    /// Set a global variable (typically from response handlers)
    pub fn set_global(&mut self, name: String, value: String) {
        self.global_variables.insert(name, value);
    }

    /// Get a global variable
    pub fn get_global(&self, name: &str) -> Option<&String> {
        self.global_variables.get(name)
    }

    /// Set a file-level variable
    pub fn set_file_variable(&mut self, name: String, value: String) {
        self.file_variables.insert(name, value);
    }

    /// Clear file-level variables (call when switching to a new file)
    pub fn clear_file_variables(&mut self) {
        self.file_variables.clear();
    }

    /// Resolve a variable by name using the resolution order:
    /// 1. File-level variables
    /// 2. Global variables (from response handlers)
    /// 3. Active environment variables
    /// 4. System environment variables
    pub fn get_variable(&self, name: &str) -> Option<String> {
        // 1. File-level variables
        if let Some(value) = self.file_variables.get(name) {
            return Some(value.clone());
        }

        // 2. Global variables
        if let Some(value) = self.global_variables.get(name) {
            return Some(value.clone());
        }

        // 3. Active environment variables
        if let Some(env_name) = &self.active_environment {
            if let Some(env) = self.environments.get(env_name) {
                if let Some(value) = env.variables.get(name) {
                    return Some(value.clone());
                }
            }
        }

        // 4. System environment variables
        if let Ok(value) = env::var(name) {
            return Some(value);
        }

        None
    }

    /// Resolve all variables in a template string
    /// Supports {{variable}} syntax
    /// Supports nested resolution: {{base_url}}/{{endpoint}}
    pub fn resolve(&self, template: &str) -> String {
        let mut result = template.to_string();
        let mut iterations = 0;
        const MAX_ITERATIONS: usize = 10; // Prevent infinite loops

        // Keep resolving until no more variables found or max iterations reached
        loop {
            if iterations >= MAX_ITERATIONS {
                break;
            }

            let before = result.clone();
            result = self.resolve_once(&result);

            // If nothing changed, we're done
            if result == before {
                break;
            }

            iterations += 1;
        }

        result
    }

    /// Perform one pass of variable resolution
    fn resolve_once(&self, template: &str) -> String {
        let mut result = String::new();
        let mut chars = template.chars().peekable();

        while let Some(ch) = chars.next() {
            if ch == '{' && chars.peek() == Some(&'{') {
                // Start of variable reference
                chars.next(); // consume second '{'

                let mut var_name = String::new();
                let mut found_closing = false;

                // Collect variable name
                while let Some(ch) = chars.next() {
                    if ch == '}' && chars.peek() == Some(&'}') {
                        chars.next(); // consume second '}'
                        found_closing = true;
                        break;
                    }
                    var_name.push(ch);
                }

                if found_closing {
                    // Resolve the variable
                    if let Some(value) = self.get_variable(var_name.trim()) {
                        result.push_str(&value);
                    } else {
                        // Variable not found, keep original syntax
                        result.push_str("{{");
                        result.push_str(&var_name);
                        result.push_str("}}");
                    }
                } else {
                    // Malformed variable reference, keep as-is
                    result.push_str("{{");
                    result.push_str(&var_name);
                }
            } else if ch == '\\' && chars.peek() == Some(&'{') {
                // Escaped variable: \{{not_a_variable}}
                result.push(ch);
                if let Some(next) = chars.next() {
                    result.push(next);
                }
            } else {
                result.push(ch);
            }
        }

        result
    }

    /// Get all available variables (for debugging/display)
    pub fn get_all_variables(&self) -> HashMap<String, String> {
        let mut all = HashMap::new();

        // Start with system env vars (lowest priority)
        for (key, value) in env::vars() {
            all.insert(key, value);
        }

        // Add environment variables
        if let Some(env_name) = &self.active_environment {
            if let Some(env) = self.environments.get(env_name) {
                for (key, value) in &env.variables {
                    all.insert(key.clone(), value.clone());
                }
            }
        }

        // Add global variables
        for (key, value) in &self.global_variables {
            all.insert(key.clone(), value.clone());
        }

        // Add file variables (highest priority)
        for (key, value) in &self.file_variables {
            all.insert(key.clone(), value.clone());
        }

        all
    }

    pub fn get_environment_variables(&self) -> HashMap<String, String> {
        let mut vars = HashMap::new();
        if let Some(env_name) = &self.active_environment {
            if let Some(env) = self.environments.get(env_name) {
                vars.clone_from(&env.variables);
            }
        }
        // If no activeEnv, still expose union of all env keys (for completion / fallback)
        if vars.is_empty() {
            for env in self.environments.values() {
                for (k, v) in &env.variables {
                    vars.entry(k.clone()).or_insert_with(|| v.clone());
                }
            }
        }
        vars
    }

    /// All variables from all environments for autocomplete.
    /// Returns (name, value, source) where source is e.g. `env[dev]` or `env[dev★]`.
    pub fn get_completion_env_variables(&self) -> Vec<(String, String, String)> {
        let mut out = Vec::new();
        let mut seen = std::collections::HashSet::new();
        let active = self.active_environment.as_deref();

        // Active environment first
        if let Some(env_name) = active {
            if let Some(env) = self.environments.get(env_name) {
                for (k, v) in &env.variables {
                    seen.insert(k.clone());
                    out.push((
                        k.clone(),
                        v.clone(),
                        format!("http-client.env.json [{env_name}] (active)"),
                    ));
                }
            }
        }

        // Then other environments (same key only once — prefer active)
        let mut names: Vec<_> = self.environments.keys().cloned().collect();
        names.sort();
        for env_name in names {
            if Some(env_name.as_str()) == active {
                continue;
            }
            if let Some(env) = self.environments.get(&env_name) {
                for (k, v) in &env.variables {
                    if seen.contains(k) {
                        continue;
                    }
                    seen.insert(k.clone());
                    out.push((
                        k.clone(),
                        v.clone(),
                        format!("http-client.env.json [{env_name}]"),
                    ));
                }
            }
        }
        out.sort_by(|a, b| a.0.cmp(&b.0));
        out
    }

    pub fn get_available_environment_names(&self) -> Vec<String> {
        self.environments.keys().cloned().collect()
    }

    /// Return the active environment name (set via `activeEnv` in http-client.env.json).
    pub fn get_default_environment_name(&self) -> Option<String> {
        self.active_environment.clone()
    }

    /// Parse file-level variable declarations from .http file content
    /// Format: @variable = value
    pub fn parse_file_variables(&mut self, content: &str) {
        self.clear_file_variables();

        for line in content.lines() {
            let trimmed = line.trim();

            // Check for variable declaration: @variable = value
            if trimmed.starts_with('@') && trimmed.contains('=') {
                if let Some(eq_pos) = trimmed.find('=') {
                    let name = trimmed[1..eq_pos].trim();
                    let value = trimmed[eq_pos + 1..].trim();

                    if !name.is_empty() && !value.is_empty() {
                        self.set_file_variable(name.to_string(), value.to_string());
                    }
                }
            }
        }
    }
}

/// Walk up from `start_dir` looking for `http-client.env.json`.
/// Returns the nearest directory that contains it, or None.
#[allow(dead_code)]
pub fn find_env_files_walking_up(start_dir: &Path) -> Option<std::path::PathBuf> {
    find_all_env_dirs_walking_up(start_dir).into_iter().next()
}

/// All directories from `start_dir` upward that contain env JSON files (near → root).
pub fn find_all_env_dirs_walking_up(start_dir: &Path) -> Vec<std::path::PathBuf> {
    let mut dirs = Vec::new();
    let mut dir = start_dir.to_path_buf();
    loop {
        if dir.join(ENV_FILE_PUBLIC).exists() || dir.join(ENV_FILE_PRIVATE).exists() {
            dirs.push(dir.clone());
        }
        if !dir.pop() {
            break;
        }
    }
    dirs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_variable_resolution_order() {
        let mut resolver = VariableResolver::new();

        // Set up different levels
        env::set_var("TEST_VAR", "from_system");

        let mut env = Environment {
            variables: HashMap::new(),
        };
        env.variables
            .insert("TEST_VAR".to_string(), "from_environment".to_string());
        resolver.environments.insert("dev".to_string(), env);
        resolver.set_active_environment("dev".to_string()).unwrap();

        resolver.set_global("TEST_VAR".to_string(), "from_global".to_string());
        resolver.set_file_variable("TEST_VAR".to_string(), "from_file".to_string());

        // File-level should win
        assert_eq!(
            resolver.get_variable("TEST_VAR"),
            Some("from_file".to_string())
        );

        // Remove file-level, global should win
        resolver.clear_file_variables();
        assert_eq!(
            resolver.get_variable("TEST_VAR"),
            Some("from_global".to_string())
        );

        // Remove global, environment should win
        resolver.global_variables.clear();
        assert_eq!(
            resolver.get_variable("TEST_VAR"),
            Some("from_environment".to_string())
        );

        env::remove_var("TEST_VAR");
    }

    #[test]
    fn test_simple_variable_substitution() {
        let mut resolver = VariableResolver::new();
        resolver.set_file_variable("name".to_string(), "John".to_string());
        resolver.set_file_variable("age".to_string(), "30".to_string());

        let result = resolver.resolve("Hello {{name}}, you are {{age}} years old");
        assert_eq!(result, "Hello John, you are 30 years old");
    }

    #[test]
    fn test_nested_variable_resolution() {
        let mut resolver = VariableResolver::new();
        resolver.set_file_variable("host".to_string(), "api.example.com".to_string());
        resolver.set_file_variable("base_url".to_string(), "https://{{host}}".to_string());
        resolver.set_file_variable("endpoint".to_string(), "users".to_string());

        let result = resolver.resolve("{{base_url}}/{{endpoint}}");
        assert_eq!(result, "https://api.example.com/users");
    }

    #[test]
    fn test_undefined_variable() {
        let resolver = VariableResolver::new();

        let result = resolver.resolve("Hello {{undefined_var}}");
        assert_eq!(result, "Hello {{undefined_var}}");
    }

    #[test]
    fn test_parse_file_variables() {
        let mut resolver = VariableResolver::new();

        let content = r#"
@base_url = https://api.example.com
@api_key = secret_key_123
@timeout = 5000

GET {{base_url}}/users
"#;

        resolver.parse_file_variables(content);

        assert_eq!(
            resolver.get_variable("base_url"),
            Some("https://api.example.com".to_string())
        );
        assert_eq!(
            resolver.get_variable("api_key"),
            Some("secret_key_123".to_string())
        );
        assert_eq!(resolver.get_variable("timeout"), Some("5000".to_string()));
    }

    #[test]
    fn test_escaped_variables() {
        let mut resolver = VariableResolver::new();
        resolver.set_file_variable("var".to_string(), "value".to_string());

        // Note: This tests that escaped brackets are preserved
        let result = resolver.resolve(r"\{{not_a_var}} but {{var}} is");
        assert!(result.contains("{{not_a_var}}") || result.contains(r"\{{not_a_var}}"));
        assert!(result.contains("value"));
    }

    #[test]
    fn test_completion_env_vars_include_active_and_others() {
        let mut resolver = VariableResolver::new();
        let mut dev = Environment {
            variables: HashMap::new(),
        };
        dev.variables
            .insert("base_url".to_string(), "https://dev.example".to_string());
        dev.variables
            .insert("only_dev".to_string(), "1".to_string());
        let mut prod = Environment {
            variables: HashMap::new(),
        };
        prod.variables
            .insert("base_url".to_string(), "https://prod.example".to_string());
        prod.variables
            .insert("only_prod".to_string(), "2".to_string());
        resolver.environments.insert("dev".to_string(), dev);
        resolver.environments.insert("prod".to_string(), prod);
        resolver.set_active_environment("dev".to_string()).unwrap();

        let comps = resolver.get_completion_env_variables();
        let names: Vec<_> = comps.iter().map(|(n, _, _)| n.as_str()).collect();
        assert!(names.contains(&"base_url"));
        assert!(names.contains(&"only_dev"));
        assert!(names.contains(&"only_prod"));
        let base = comps.iter().find(|(n, _, _)| n == "base_url").unwrap();
        assert!(base.2.contains("active"));
        assert_eq!(base.1, "https://dev.example");
    }

    #[test]
    fn test_load_environments_json() {
        let json_content = r#"{
            "activeEnv": "development",
            "development": {
                "host": "localhost:3000",
                "api_key": "dev_key"
            },
            "production": {
                "host": "api.example.com",
                "api_key": "prod_key"
            }
        }"#;

        let temp_file = std::env::temp_dir().join("test_environments.json");
        fs::write(&temp_file, json_content).unwrap();

        let mut resolver = VariableResolver::new();
        resolver.load_environments(&temp_file).unwrap();

        // activeEnv sets the active environment automatically
        assert_eq!(
            resolver.get_active_environment(),
            Some(&"development".to_string())
        );
        assert_eq!(
            resolver.get_variable("host"),
            Some("localhost:3000".to_string())
        );

        resolver
            .set_active_environment("production".to_string())
            .unwrap();
        assert_eq!(
            resolver.get_variable("host"),
            Some("api.example.com".to_string())
        );

        fs::remove_file(&temp_file).ok();
    }

    #[test]
    fn test_load_environments_from_dir_with_private_override() {
        let temp_dir = std::env::temp_dir().join("test_jetbrains_env");
        let _ = fs::create_dir_all(&temp_dir);

        let public = r#"{
            "activeEnv": "dev",
            "dev": { "host": "localhost", "api_key": "public-key" },
            "prod": { "host": "prod.example.com", "api_key": "public-prod-key" }
        }"#;
        fs::write(temp_dir.join(ENV_FILE_PUBLIC), public).unwrap();

        let private = r#"{
            "dev": { "api_key": "secret-dev-key", "password": "s3cret" }
        }"#;
        fs::write(temp_dir.join(ENV_FILE_PRIVATE), private).unwrap();

        let mut resolver = VariableResolver::new();
        assert!(resolver.load_environments_from_dir(&temp_dir));

        assert_eq!(resolver.get_variable("host"), Some("localhost".to_string()));
        assert_eq!(
            resolver.get_variable("api_key"),
            Some("secret-dev-key".to_string())
        );
        assert_eq!(
            resolver.get_variable("password"),
            Some("s3cret".to_string())
        );

        resolver.set_active_environment("prod".to_string()).unwrap();
        assert_eq!(
            resolver.get_variable("api_key"),
            Some("public-prod-key".to_string())
        );

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_active_env_from_json() {
        let temp_dir = std::env::temp_dir().join("test_active_env");
        let _ = fs::create_dir_all(&temp_dir);

        let public = r#"{
            "activeEnv": "staging",
            "development": { "host": "localhost" },
            "staging": { "host": "staging.example.com" },
            "production": { "host": "prod.example.com" }
        }"#;
        fs::write(temp_dir.join(ENV_FILE_PUBLIC), public).unwrap();

        let mut resolver = VariableResolver::new();
        assert!(resolver.load_environments_from_dir(&temp_dir));

        assert_eq!(
            resolver.get_active_environment(),
            Some(&"staging".to_string())
        );
        assert_eq!(
            resolver.get_variable("host"),
            Some("staging.example.com".to_string())
        );

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_active_env_invalid_name_errors() {
        let json_content = r#"{
            "activeEnv": "nonexistent",
            "development": { "host": "localhost" }
        }"#;

        let temp_file = std::env::temp_dir().join("test_active_env_invalid.json");
        fs::write(&temp_file, json_content).unwrap();

        let mut resolver = VariableResolver::new();
        let result = resolver.load_environments(&temp_file);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("nonexistent"));

        fs::remove_file(&temp_file).ok();
    }
}
