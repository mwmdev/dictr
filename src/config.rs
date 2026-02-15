use anyhow::Result;
use serde::Deserialize;
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Deserialize)]
pub struct Config {
    #[serde(default = "default_hotkey")]
    pub hotkey: String,
    #[serde(default = "default_backend")]
    pub backend: String,
    #[serde(default = "default_model_path")]
    pub model_path: String,
    #[serde(default)]
    pub api_key: String,
    #[serde(default = "default_api_url")]
    pub api_url: String,
    #[serde(default = "default_typing_delay")]
    pub typing_delay_ms: u64,
    #[serde(default = "default_min_duration")]
    pub min_duration_ms: u64,
    #[serde(default)]
    pub device: Option<String>,
    #[serde(default)]
    pub language: Option<String>,
    #[serde(default)]
    pub initial_prompt: Option<String>,
    #[serde(default)]
    pub replacements: Replacements,
}

#[derive(Debug, Deserialize)]
pub struct Replacements {
    #[serde(default = "default_true")]
    pub lowercase_after: bool,
    #[serde(flatten)]
    pub rules: HashMap<String, String>,
}

fn default_true() -> bool {
    true
}

impl Default for Replacements {
    fn default() -> Self {
        Self {
            lowercase_after: true,
            rules: HashMap::new(),
        }
    }
}

fn default_hotkey() -> String {
    "AltGr".into()
}
fn default_backend() -> String {
    "local".into()
}
fn default_model_path() -> String {
    "~/.local/share/dictr/models/ggml-base.bin".into()
}
fn default_api_url() -> String {
    "https://api.openai.com/v1/audio/transcriptions".into()
}
fn default_typing_delay() -> u64 {
    2
}
fn default_min_duration() -> u64 {
    300
}

impl Default for Config {
    fn default() -> Self {
        Self {
            hotkey: default_hotkey(),
            backend: default_backend(),
            model_path: default_model_path(),
            api_key: String::new(),
            api_url: default_api_url(),
            typing_delay_ms: default_typing_delay(),
            min_duration_ms: default_min_duration(),
            device: None,
            language: None,
            initial_prompt: None,
            replacements: Replacements::default(),
        }
    }
}

impl Config {
    pub fn load() -> Result<Self> {
        let path = config_path();
        let mut config = if path.exists() {
            let contents = std::fs::read_to_string(&path)?;
            toml::from_str::<Config>(&contents)?
        } else {
            Config::default()
        };
        config.resolve_env();
        Ok(config)
    }

    fn resolve_env(&mut self) {
        // Expand tilde in model_path
        if self.model_path.starts_with('~') {
            if let Some(home) = std::env::var_os("HOME") {
                self.model_path = self.model_path.replacen('~', &home.to_string_lossy(), 1);
            }
        }

        // Env var fallback for API key
        if self.api_key.is_empty() {
            if let Ok(key) = std::env::var("OPENAI_API_KEY") {
                self.api_key = key;
            }
        }
    }

    pub fn resolved_model_path(&self) -> PathBuf {
        PathBuf::from(&self.model_path)
    }

    pub fn apply_replacements(&self, text: &str) -> String {
        let mut result = text.to_string();
        for (from, to) in &self.replacements.rules {
            if from.is_empty() {
                continue;
            }
            let from_lower = from.to_lowercase();
            let mut out = String::new();
            let mut remaining = result.as_str();
            while let Some(pos) = remaining.to_lowercase().find(&from_lower) {
                out.push_str(&remaining[..pos]);
                out.push_str(to);
                remaining = &remaining[pos + from.len()..];
            }
            out.push_str(remaining);
            result = out;
        }
        if self.replacements.lowercase_after {
            let prefixes: Vec<&str> = self
                .replacements
                .rules
                .values()
                .map(|v| v.as_str())
                .filter(|v| {
                    v.ends_with(|c: char| {
                        !c.is_whitespace() && !matches!(c, '.' | ',' | ';' | ':' | '!' | '?')
                    })
                })
                .collect();
            for prefix in prefixes {
                let mut out = String::new();
                let mut remaining = result.as_str();
                while let Some(pos) = remaining.find(prefix) {
                    let end = pos + prefix.len();
                    out.push_str(&remaining[..end]);
                    remaining = &remaining[end..];
                    // Strip whitespace between prefix and word
                    remaining = remaining.trim_start();
                    // Grab and lowercase the word
                    let word_end = remaining
                        .find(|c: char| !c.is_alphanumeric())
                        .unwrap_or(remaining.len());
                    out.push_str(&remaining[..word_end].to_lowercase());
                    remaining = &remaining[word_end..];
                    // Strip trailing punctuation
                    remaining = remaining.trim_start_matches(['.', ',', '!', '?']);
                    // Ensure exactly one space before next content
                    if !remaining.is_empty() {
                        remaining = remaining.trim_start_matches(' ');
                        out.push(' ');
                    }
                }
                out.push_str(remaining);
                result = out;
            }
        }
        result
    }
}

fn config_path() -> PathBuf {
    let mut path = dirs_path();
    path.push("config.toml");
    path
}

fn dirs_path() -> PathBuf {
    if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
        PathBuf::from(xdg).join("dictr")
    } else if let Some(home) = std::env::var_os("HOME") {
        PathBuf::from(home).join(".config").join("dictr")
    } else {
        PathBuf::from(".config/dictr")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_sane() {
        let config = Config::default();
        assert_eq!(config.hotkey, "AltGr");
        assert_eq!(config.backend, "local");
        assert_eq!(config.typing_delay_ms, 2);
        assert!(config.api_key.is_empty());
        assert!(config.model_path.contains("ggml-base.bin"));
    }

    #[test]
    fn parse_full_toml() {
        let toml = r#"
            hotkey = "F9"
            backend = "api"
            model_path = "/tmp/model.bin"
            api_key = "sk-test"
            typing_delay_ms = 5
        "#;
        let config: Config = toml::from_str(toml).unwrap();
        assert_eq!(config.hotkey, "F9");
        assert_eq!(config.backend, "api");
        assert_eq!(config.model_path, "/tmp/model.bin");
        assert_eq!(config.api_key, "sk-test");
        assert_eq!(config.typing_delay_ms, 5);
    }

    #[test]
    fn parse_partial_toml_uses_defaults() {
        let toml = r#"hotkey = "CapsLock""#;
        let config: Config = toml::from_str(toml).unwrap();
        assert_eq!(config.hotkey, "CapsLock");
        assert_eq!(config.backend, "local");
        assert_eq!(config.typing_delay_ms, 2);
    }

    #[test]
    fn parse_empty_toml() {
        let config: Config = toml::from_str("").unwrap();
        assert_eq!(config.hotkey, "AltGr");
        assert_eq!(config.backend, "local");
    }

    #[test]
    fn env_var_fills_empty_api_key() {
        let mut config: Config = toml::from_str("").unwrap();
        assert!(config.api_key.is_empty());

        std::env::set_var("OPENAI_API_KEY", "sk-from-env");
        config.resolve_env();
        assert_eq!(config.api_key, "sk-from-env");
        std::env::remove_var("OPENAI_API_KEY");
    }

    #[test]
    fn toml_api_key_not_overridden_by_env() {
        let mut config: Config = toml::from_str(r#"api_key = "sk-from-toml""#).unwrap();

        std::env::set_var("OPENAI_API_KEY", "sk-from-env");
        config.resolve_env();
        assert_eq!(config.api_key, "sk-from-toml");
        std::env::remove_var("OPENAI_API_KEY");
    }

    #[test]
    fn tilde_expansion() {
        let mut config: Config = toml::from_str(r#"model_path = "~/models/test.bin""#).unwrap();
        config.resolve_env();
        assert!(!config.model_path.starts_with('~'));
        assert!(config.model_path.ends_with("/models/test.bin"));
    }

    #[test]
    fn absolute_path_not_expanded() {
        let mut config: Config = toml::from_str(r#"model_path = "/opt/models/test.bin""#).unwrap();
        config.resolve_env();
        assert_eq!(config.model_path, "/opt/models/test.bin");
    }

    #[test]
    fn replacements_basic() {
        let toml = r#"
            [replacements]
            "slash " = "/"
            "new line" = "\n"
        "#;
        let config: Config = toml::from_str(toml).unwrap();
        assert_eq!(config.apply_replacements("slash review"), "/review");
        assert_eq!(
            config.apply_replacements("add new line here"),
            "add \n here"
        );
    }

    #[test]
    fn replacements_case_insensitive() {
        let toml = r#"
            [replacements]
            "slash " = "/"
        "#;
        let config: Config = toml::from_str(toml).unwrap();
        assert_eq!(config.apply_replacements("Slash review"), "/review");
        assert_eq!(config.apply_replacements("SLASH commit"), "/commit");
    }

    #[test]
    fn replacements_multiple_occurrences() {
        let toml = r#"
            [replacements]
            "comma" = ","
        "#;
        let config: Config = toml::from_str(toml).unwrap();
        assert_eq!(
            config.apply_replacements("one comma two comma three"),
            "one , two , three"
        );
    }

    #[test]
    fn lowercase_after_default_on() {
        let toml = r#"
            [replacements]
            "slash " = "/"
        "#;
        let config: Config = toml::from_str(toml).unwrap();
        assert!(config.replacements.lowercase_after);
        // Basic: lowercase and no space after prefix
        assert_eq!(config.apply_replacements("Slash Commit"), "/commit");
        // Multiple occurrences
        assert_eq!(
            config.apply_replacements("Slash Home Slash Documents"),
            "/home /documents"
        );
        // Strips trailing punctuation
        assert_eq!(config.apply_replacements("Slash Commit."), "/commit");
        assert_eq!(
            config.apply_replacements("Slash Commit, done"),
            "/commit done"
        );
        // Strips space between prefix and word
        assert_eq!(config.apply_replacements("Slash  Commit"), "/commit");
        // No match passthrough
        assert_eq!(
            config.apply_replacements("no prefix here"),
            "no prefix here"
        );
    }

    #[test]
    fn lowercase_after_disabled() {
        let toml = r#"
            [replacements]
            lowercase_after = false
            "slash " = "/"
        "#;
        let config: Config = toml::from_str(toml).unwrap();
        assert!(!config.replacements.lowercase_after);
        assert_eq!(config.apply_replacements("Slash Commit"), "/Commit");
    }

    #[test]
    fn replacements_empty_map_passthrough() {
        let config = Config::default();
        assert_eq!(config.apply_replacements("hello world"), "hello world");
    }

    #[test]
    fn replacements_empty_key_skipped() {
        // An empty key should not cause an infinite loop
        let mut config = Config::default();
        config.replacements.rules.insert(String::new(), "X".into());
        assert_eq!(config.apply_replacements("hello"), "hello");
    }

    #[test]
    fn dirs_path_xdg_and_fallback() {
        // Test XDG_CONFIG_HOME override
        std::env::set_var("XDG_CONFIG_HOME", "/tmp/xdg-test");
        let path = dirs_path();
        assert_eq!(path, PathBuf::from("/tmp/xdg-test/dictr"));

        // Test HOME fallback when XDG is unset
        std::env::remove_var("XDG_CONFIG_HOME");
        let path = dirs_path();
        let home = std::env::var("HOME").unwrap();
        assert_eq!(path, PathBuf::from(home).join(".config").join("dictr"));
    }

    #[test]
    fn config_path_ends_with_config_toml() {
        let path = config_path();
        assert!(path.ends_with("config.toml"));
        assert!(path.parent().unwrap().ends_with("dictr"));
    }

    #[test]
    fn parse_new_config_fields() {
        let toml = r#"
            api_url = "http://localhost:8080/v1/transcriptions"
            min_duration_ms = 500
            initial_prompt = "NixOS, Rust"
            language = "en"
        "#;
        let config: Config = toml::from_str(toml).unwrap();
        assert_eq!(config.api_url, "http://localhost:8080/v1/transcriptions");
        assert_eq!(config.min_duration_ms, 500);
        assert_eq!(config.initial_prompt, Some("NixOS, Rust".into()));
        assert_eq!(config.language, Some("en".into()));
    }

    #[test]
    fn new_config_fields_have_defaults() {
        let config: Config = toml::from_str("").unwrap();
        assert_eq!(
            config.api_url,
            "https://api.openai.com/v1/audio/transcriptions"
        );
        assert_eq!(config.min_duration_ms, 300);
        assert!(config.initial_prompt.is_none());
        assert!(config.language.is_none());
    }
}
