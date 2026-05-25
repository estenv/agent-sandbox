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
        &[],
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

    let deny_read = parsed
        .pointer("/filesystem/denyRead")
        .unwrap()
        .as_array()
        .unwrap();
    assert_eq!(deny_read.len(), 1, "only \"~\" should be in denyRead");
    let home = deny_read[0].as_str().unwrap();
    assert!(
        home.starts_with('/'),
        "\"~\" should be expanded to an absolute path, got: {home}"
    );
    assert!(
        home.contains('/'),
        "home path should contain slashes, got: {home}"
    );

    let allow_write = parsed
        .pointer("/filesystem/allowWrite")
        .unwrap()
        .as_array()
        .unwrap();
    let has_workspace = allow_write
        .iter()
        .any(|v| v.as_str().is_some_and(|s| s.ends_with("/.agent-sandbox")));
    assert!(
        has_workspace,
        "~/.agent-sandbox should be expanded to an absolute path"
    );
}

#[test]
fn test_prepare_settings_deny_read_blocks_home() {
    let parsed = render_settings(
        "/tmp/proj",
        "/tmp/d.sock",
        &["api.anthropic.com".to_string()],
    );

    let deny_read = parsed
        .pointer("/filesystem/denyRead")
        .unwrap()
        .as_array()
        .unwrap();
    assert_eq!(deny_read.len(), 1, "only ~ should be in denyRead");
    let home: String = deny_read[0].as_str().unwrap().to_string();
    assert!(
        home.starts_with('/'),
        "home path must be absolute, got: {home}"
    );
}

#[test]
fn test_prepare_settings_allow_read_contains_tool_paths() {
    let parsed = render_settings(
        "/tmp/proj",
        "/tmp/d.sock",
        &["api.anthropic.com".to_string()],
    );

    let allow_read = parsed
        .pointer("/filesystem/allowRead")
        .unwrap()
        .as_array()
        .unwrap();
    assert!(
        !allow_read.is_empty(),
        "allowRead should contain discovered tool paths"
    );
    for entry in allow_read {
        let s = entry.as_str().unwrap();
        assert!(
            s.starts_with('/'),
            "allowRead entry should be absolute: {s}"
        );
    }
}

#[test]
fn test_prepare_settings_allow_write_contains_cargo() {
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
    let has_cargo = allow_write
        .iter()
        .any(|v| v.as_str().is_some_and(|s| s.contains("/.cargo")));
    assert!(has_cargo, "allowWrite should contain ~/.cargo path");
}

#[test]
fn test_prepare_settings_allow_write_contains_agent_paths() {
    let parsed = render_settings(
        "/tmp/proj",
        "/tmp/d.sock",
        &["api.anthropic.com".to_string()],
    );

    let allow_write: Vec<String> = parsed
        .pointer("/filesystem/allowWrite")
        .unwrap()
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|v| v.as_str().map(String::from))
        .collect();

    let has_pi = allow_write.iter().any(|s| s.contains("/.pi"));
    assert!(has_pi, "allowWrite should contain ~/.pi path");

    let has_opencode_config = allow_write.iter().any(|s| s.contains("/.config/opencode"));
    assert!(
        has_opencode_config,
        "allowWrite should contain ~/.config/opencode path"
    );

    let has_opencode_share = allow_write
        .iter()
        .any(|s| s.contains("/.local/share/opencode"));
    assert!(
        has_opencode_share,
        "allowWrite should contain ~/.local/share/opencode path"
    );
}

fn render_settings_with_extra(
    projects_root: &str,
    daemon_sock: &str,
    allowed_domains: &[String],
    extra_write_dirs: &[PathBuf],
) -> serde_json::Value {
    agent_sandbox::policy::render_settings(
        &PathBuf::from(projects_root),
        &PathBuf::from(daemon_sock),
        allowed_domains,
        extra_write_dirs,
    )
    .unwrap()
}

#[test]
fn test_prepare_settings_extra_write_dirs_in_both_lists() {
    let extra = vec![PathBuf::from("/home/user/agent-configs")];
    let parsed = render_settings_with_extra("/tmp/proj", "/tmp/d.sock", &[], &extra);

    let allow_read = parsed
        .pointer("/filesystem/allowRead")
        .unwrap()
        .as_array()
        .unwrap();
    assert!(
        allow_read.contains(&serde_json::Value::String(
            "/home/user/agent-configs".to_string()
        )),
        "extra dir should appear in allowRead"
    );

    let allow_write = parsed
        .pointer("/filesystem/allowWrite")
        .unwrap()
        .as_array()
        .unwrap();
    assert!(
        allow_write.contains(&serde_json::Value::String(
            "/home/user/agent-configs".to_string()
        )),
        "extra dir should appear in allowWrite"
    );
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
