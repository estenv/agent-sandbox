use serde::{Deserialize, Serialize};
use std::env;
use std::path::PathBuf;

pub const DEFAULT_CONFIG_JSON: &str = r#"{
  "projects_root": "~/repos",
  "sandbox_home": "~/.agent-sandbox"
}"#;

#[derive(Debug, Deserialize, Serialize)]
pub struct WrapperConfig {
    pub projects_root: String,
    pub sandbox_home: String,
}

pub fn init() -> Result<u8, Box<dyn std::error::Error>> {
    let config_dir = config_dir()?;
    let config_path = config_dir.join("config.json");
    let workspace = resolve_path("~/.agent-sandbox")?;

    std::fs::create_dir_all(&config_dir)?;
    crate::sandbox::ensure_workspace_dirs(&workspace)?;

    if !config_path.exists() {
        std::fs::write(&config_path, DEFAULT_CONFIG_JSON)?;
        println!("created {}", config_path.display());
    } else {
        println!("config already exists: {}", config_path.display());
    }

    println!("sandbox workspace: {}", workspace.display());
    Ok(0)
}

pub fn load_config() -> Result<WrapperConfig, Box<dyn std::error::Error>> {
    let config_dir = config_dir()?;
    let config_path = config_dir.join("config.json");

    let mut config = if config_path.exists() {
        let content = std::fs::read_to_string(&config_path)?;
        serde_json::from_str(&content)?
    } else {
        WrapperConfig {
            projects_root: "~/repos".to_string(),
            sandbox_home: "~/.agent-sandbox".to_string(),
        }
    };

    if let Some(val) = env::var_os("AGENT_SANDBOX_PROJECTS_ROOT") {
        config.projects_root = val.to_string_lossy().to_string();
    }
    if let Some(val) = env::var_os("AGENT_SANDBOX_HOME") {
        config.sandbox_home = val.to_string_lossy().to_string();
    }

    Ok(config)
}

pub fn config_dir() -> Result<PathBuf, Box<dyn std::error::Error>> {
    if let Some(xdg) = env::var_os("XDG_CONFIG_HOME") {
        return Ok(PathBuf::from(xdg).join("agent-sandbox"));
    }
    Ok(home_dir()?.join(".config/agent-sandbox"))
}

pub fn resolve_path(path: &str) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let expanded = if let Some(rest) = path.strip_prefix('~') {
        let home = home_dir()?;
        if rest.is_empty() || rest == "/" {
            home
        } else {
            let rest = rest.trim_start_matches('/');
            home.join(rest)
        }
    } else {
        PathBuf::from(path)
    };
    if expanded.is_relative() {
        Ok(env::current_dir()?.join(expanded))
    } else {
        Ok(expanded)
    }
}

fn home_dir() -> Result<PathBuf, Box<dyn std::error::Error>> {
    env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or_else(|| "HOME is not set".into())
}

