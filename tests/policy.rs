use std::path::PathBuf;

fn run_prepare_settings(
    projects_root: &str,
    daemon_sock: &str,
) -> (String, serde_json::Value) {
    let path = agent_sandbox::policy::prepare_settings(
        &PathBuf::from(projects_root),
        &PathBuf::from(daemon_sock),
    )
    .unwrap();
    let content = std::fs::read_to_string(&path).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
    (content, parsed)
}

#[test]
fn test_prepare_settings_writes_valid_json() {
    let (_, parsed) = run_prepare_settings("/tmp/proj", "/tmp/daemon/test.sock");
    assert!(parsed.pointer("/network").is_some());
    assert!(parsed.pointer("/filesystem").is_some());
    assert!(parsed.pointer("/ignoreViolations").is_some());
}

#[test]
fn test_prepare_settings_replaces_dot_with_projects_root() {
    let (_, parsed) = run_prepare_settings("/tmp/test-projects", "/tmp/daemon/test.sock");
    let allow_write = parsed.pointer("/filesystem/allowWrite").unwrap().as_array().unwrap();
    assert!(allow_write.contains(&serde_json::Value::String("/tmp/test-projects".to_string())));
    assert!(!allow_write.contains(&serde_json::Value::String(".".to_string())));
}

#[test]
fn test_prepare_settings_adds_daemon_socket_dir_to_allow_write() {
    let (_, parsed) = run_prepare_settings("/tmp/proj", "/tmp/daemon-dir/test.sock");
    let allow_write = parsed.pointer("/filesystem/allowWrite").unwrap().as_array().unwrap();
    assert!(allow_write.contains(&serde_json::Value::String("/tmp/daemon-dir".to_string())));
}

#[test]
fn test_prepare_settings_no_duplicate_daemon_socket_dir() {
    let path = agent_sandbox::policy::prepare_settings(
        &PathBuf::from("/tmp/proj"),
        &PathBuf::from("/tmp/daemon/test.sock"),
    )
    .unwrap();
    let content = std::fs::read_to_string(&path).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
    let count = parsed
        .pointer("/filesystem/allowWrite")
        .unwrap()
        .as_array()
        .unwrap()
        .iter()
        .filter(|v| v.as_str() == Some("/tmp/daemon"))
        .count();
    assert_eq!(count, 1, "daemon socket dir should appear exactly once");
}

#[test]
fn test_prepare_settings_expands_tilde_paths() {
    let (_, parsed) = run_prepare_settings("/tmp/proj", "/tmp/d.sock");

    let allow_write = parsed.pointer("/filesystem/allowWrite").unwrap().as_array().unwrap();
    let expanded: Vec<&str> = allow_write.iter().filter_map(|v| v.as_str()).collect();
    assert!(
        expanded.iter().any(|p| p.ends_with("/.agent-sandbox")),
        "~/.agent-sandbox should be expanded to an absolute path, got: {expanded:?}"
    );

    let deny_read = parsed.pointer("/filesystem/denyRead").unwrap().as_array().unwrap();
    let has_abs_ssh = deny_read
        .iter()
        .any(|v| v.as_str().map_or(false, |s| s.starts_with('/') && s.contains("/.ssh")));
    assert!(has_abs_ssh, "~/.ssh should be expanded to an absolute path");
}

#[test]
fn test_prepare_settings_network_defaults() {
    let (_, parsed) = run_prepare_settings("/tmp/p", "/tmp/d.sock");
    assert!(parsed.pointer("/network/deniedDomains").is_some());
    assert_eq!(
        parsed.pointer("/network/allowAllUnixSockets").unwrap().as_bool(),
        Some(true)
    );
    assert_eq!(
        parsed.pointer("/network/allowLocalBinding").unwrap().as_bool(),
        Some(false)
    );
}

#[test]
fn test_prepare_settings_allowed_domains_no_localhost() {
    let (_, parsed) = run_prepare_settings("/tmp/p", "/tmp/d.sock");
    let allowed = parsed.pointer("/network/allowedDomains").unwrap().as_array().unwrap();
    assert!(!allowed.contains(&serde_json::Value::String("localhost".to_string())));
    assert!(!allowed.contains(&serde_json::Value::String("127.0.0.1".to_string())));
}
