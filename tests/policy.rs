use std::path::PathBuf;

fn render_settings(
    projects_root: &str,
    daemon_sock: &str,
    allowed_domains: &[String],
) -> serde_json::Value {
    agent_sandbox::policy::render_settings(
        &PathBuf::from(projects_root),
        &PathBuf::from(daemon_sock),
        allowed_domains,
    )
    .unwrap()
}

#[test]
fn test_prepare_settings_replaces_dot_with_projects_root() {
    let parsed = render_settings(
        "/tmp/test-projects",
        "/tmp/daemon/test.sock",
        &["api.anthropic.com".to_string()],
    );
    let allow_write = parsed
        .pointer("/filesystem/allowWrite")
        .unwrap()
        .as_array()
        .unwrap();
    assert!(allow_write.contains(&serde_json::Value::String("/tmp/test-projects".to_string())));
    assert!(!allow_write.contains(&serde_json::Value::String(".".to_string())));
}

#[test]
fn test_prepare_settings_adds_daemon_socket_dir_to_allow_write() {
    let parsed = render_settings(
        "/tmp/proj",
        "/tmp/daemon-dir/test.sock",
        &["api.anthropic.com".to_string()],
    );
    let allow_write = parsed
        .pointer("/filesystem/allowWrite")
        .unwrap()
        .as_array()
        .unwrap();
    assert!(allow_write.contains(&serde_json::Value::String("/tmp/daemon-dir".to_string())));
}

#[test]
fn test_prepare_settings_no_duplicate_daemon_socket_dir() {
    let parsed = render_settings(
        "/tmp/proj",
        "/tmp/daemon/test.sock",
        &["api.anthropic.com".to_string()],
    );
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
    let parsed = render_settings(
        "/tmp/proj",
        "/tmp/d.sock",
        &["api.anthropic.com".to_string()],
    );

    let allow_write = parsed
        .pointer("/filesystem/allowWrite")
        .unwrap()
        .as_array()
        .unwrap();
    let expanded: Vec<&str> = allow_write.iter().filter_map(|v| v.as_str()).collect();
    assert!(
        expanded.iter().any(|p| p.ends_with("/.agent-sandbox")),
        "~/.agent-sandbox should be expanded to an absolute path, got: {expanded:?}"
    );

    let deny_read = parsed
        .pointer("/filesystem/denyRead")
        .unwrap()
        .as_array()
        .unwrap();
    let has_abs_ssh = deny_read.iter().any(|v| {
        v.as_str()
            .is_some_and(|s| s.starts_with('/') && s.contains("/.ssh"))
    });
    assert!(has_abs_ssh, "~/.ssh should be expanded to an absolute path");
}

#[test]
fn test_prepare_settings_custom_allowed_domains() {
    let custom = vec!["api.anthropic.com".to_string(), "github.com".to_string()];
    let parsed = render_settings("/tmp/p", "/tmp/d.sock", &custom);
    let allowed = parsed
        .pointer("/network/allowedDomains")
        .unwrap()
        .as_array()
        .unwrap();
    assert_eq!(allowed.len(), 2);
    assert!(allowed.contains(&serde_json::Value::String("api.anthropic.com".to_string())));
    assert!(allowed.contains(&serde_json::Value::String("github.com".to_string())));
}
