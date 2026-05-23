use std::path::PathBuf;

#[test]
fn test_resolve_path_tilde() {
    let home = std::env::var("HOME").unwrap();
    assert_eq!(
        agent_sandbox::config::resolve_path("~/foo").unwrap(),
        PathBuf::from(home).join("foo")
    );
}

#[test]
fn test_config_dir_uses_xdg_when_set() {
    let _guard = ScopedEnv::set("XDG_CONFIG_HOME", "/custom/xdg");
    let dir = agent_sandbox::config::config_dir().unwrap();
    assert_eq!(dir, PathBuf::from("/custom/xdg/agent-sandbox"));
}

#[test]
fn test_config_dir_default_when_xdg_unset() {
    let _guard = ScopedEnv::remove("XDG_CONFIG_HOME");
    let home = std::env::var("HOME").unwrap();
    let dir = agent_sandbox::config::config_dir().unwrap();
    assert_eq!(dir, PathBuf::from(home).join(".config/agent-sandbox"));
}

#[test]
fn test_ensure_workspace_dirs() {
    let tmp = std::env::temp_dir().join("agent-sandbox-test-workspace");
    let _ = std::fs::remove_dir_all(&tmp);
    agent_sandbox::sandbox::ensure_workspace_dirs(&tmp).unwrap();
    for name in [
        "home",
        "config",
        "cache",
        "share",
        "tmp",
        "npm-cache",
        "npm-prefix",
        "bin",
        "logs",
    ] {
        assert!(tmp.join(name).is_dir(), "missing dir: {name}");
    }
    std::fs::remove_dir_all(&tmp).unwrap();
}

// ---- scoped env guard ----

struct ScopedEnv {
    key: &'static str,
    prev: Option<String>,
}

impl ScopedEnv {
    fn set(key: &'static str, val: &str) -> Self {
        let prev = std::env::var_os(key).map(|v| v.to_string_lossy().to_string());
        std::env::set_var(key, val);
        ScopedEnv { key, prev }
    }

    fn remove(key: &'static str) -> Self {
        let prev = std::env::var_os(key).map(|v| v.to_string_lossy().to_string());
        std::env::remove_var(key);
        ScopedEnv { key, prev }
    }
}

impl Drop for ScopedEnv {
    fn drop(&mut self) {
        match &self.prev {
            Some(v) => std::env::set_var(self.key, v),
            None => std::env::remove_var(self.key),
        }
    }
}
