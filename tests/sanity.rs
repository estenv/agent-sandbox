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
fn test_resolve_path_absolute() {
    assert_eq!(
        agent_sandbox::config::resolve_path("/tmp/bar").unwrap(),
        PathBuf::from("/tmp/bar")
    );
}

#[test]
fn test_config_dir_uses_xdg_when_set() {
    let prev = std::env::var_os("XDG_CONFIG_HOME");
    std::env::set_var("XDG_CONFIG_HOME", "/custom/xdg");
    let dir = agent_sandbox::config::config_dir().unwrap();
    assert_eq!(dir, PathBuf::from("/custom/xdg/agent-sandbox"));
    match prev {
        Some(v) => std::env::set_var("XDG_CONFIG_HOME", v),
        None => std::env::remove_var("XDG_CONFIG_HOME"),
    }
}

#[test]
fn test_known_agents() {
    assert_eq!(
        agent_sandbox::agent::known_for_command("opencode"),
        Some("opencode")
    );
    assert_eq!(agent_sandbox::agent::known_for_command("pi"), Some("pi"));
    assert_eq!(
        agent_sandbox::agent::known_for_command("pi-agent"),
        Some("pi")
    );
    assert_eq!(
        agent_sandbox::agent::known_for_command("claude"),
        Some("claude")
    );
    assert_eq!(
        agent_sandbox::agent::known_for_command("copilot"),
        None
    );
    assert_eq!(agent_sandbox::agent::known_for_command("unknown"), None);
}
