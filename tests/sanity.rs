use std::path::PathBuf;

// Env-mutating tests must run serially within this file to avoid racing
// on shared global state. Rust runs tests in the same binary in parallel
// by default, so all env-dependent assertions live in one test.
#[test]
fn test_config_and_path_resolution() {
    struct EnvGuard(&'static str, Option<std::ffi::OsString>);
    impl Drop for EnvGuard {
        fn drop(&mut self) {
            match &self.1 {
                Some(v) => std::env::set_var(self.0, v),
                None => std::env::remove_var(self.0),
            }
        }
    }

    let _home = EnvGuard("HOME", std::env::var_os("HOME"));
    let _xdg = EnvGuard("XDG_CONFIG_HOME", std::env::var_os("XDG_CONFIG_HOME"));

    // resolve_path tilde expansion
    std::env::set_var("HOME", "/home/testuser");
    assert_eq!(
        agent_sandbox::config::resolve_path("~/foo").unwrap(),
        PathBuf::from("/home/testuser").join("foo")
    );

    // config_dir with XDG_CONFIG_HOME set
    std::env::set_var("XDG_CONFIG_HOME", "/custom/xdg");
    assert_eq!(
        agent_sandbox::config::config_dir().unwrap(),
        PathBuf::from("/custom/xdg/agent-sandbox")
    );

    // config_dir without XDG_CONFIG_HOME
    std::env::remove_var("XDG_CONFIG_HOME");
    std::env::set_var("HOME", "/home/testuser");
    assert_eq!(
        agent_sandbox::config::config_dir().unwrap(),
        PathBuf::from("/home/testuser/.config/agent-sandbox")
    );
}

#[test]
fn test_ensure_workspace_dirs() {
    let tmp = tempfile::TempDir::new().unwrap();
    let root = tmp.path().join("workspace");
    agent_sandbox::sandbox::ensure_workspace_dirs(&root).unwrap();
    for name in ["cache", "share", "bin"] {
        assert!(root.join(name).is_dir(), "missing dir: {name}");
    }
}
