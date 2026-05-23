use std::path::PathBuf;

#[test]
fn test_prepare_settings_expands_tilde() {
    let projects_root = PathBuf::from("/tmp/test-projects");
    let daemon_sock = PathBuf::from("/home/as/.agent-sandbox/daemon.sock");
    let result =
        agent_sandbox::policy::prepare_settings(&projects_root, &daemon_sock).unwrap();

    let content = std::fs::read_to_string(&result).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();

    // Verify tilde paths are expanded
    let allow_write = parsed
        .pointer("/filesystem/allowWrite")
        .unwrap()
        .as_array()
        .unwrap();
    let expanded_home: String = allow_write
        .iter()
        .find(|v| v.as_str().unwrap().ends_with("/.agent-sandbox"))
        .map(|v| v.as_str().unwrap().to_string())
        .expect("~/.agent-sandbox should be expanded to an absolute path");
    assert!(
        expanded_home.starts_with("/home/"),
        "path should be absolute under /home/, got: {expanded_home}"
    );

    // Verify projects_root replacement
    assert!(allow_write.contains(&serde_json::Value::String(
        "/tmp/test-projects".to_string()
    )));

    // Verify denyRead paths are expanded
    let deny_read = parsed
        .pointer("/filesystem/denyRead")
        .unwrap()
        .as_array()
        .unwrap();
    let has_abs_ssh = deny_read
        .iter()
        .any(|v| v.as_str().unwrap().contains("/.ssh"));
    assert!(
        has_abs_ssh,
        "~/.ssh should be expanded, got: {:?}",
        deny_read
    );

    // Verify network defaults
    assert!(parsed.pointer("/network/deniedDomains").is_some());
    assert_eq!(
        parsed
            .pointer("/network/allowAllUnixSockets")
            .unwrap()
            .as_bool(),
        Some(true)
    );
    assert_eq!(
        parsed
            .pointer("/network/allowLocalBinding")
            .unwrap()
            .as_bool(),
        Some(false)
    );

    // Verify localhost is NOT in allowedDomains
    let allowed = parsed
        .pointer("/network/allowedDomains")
        .unwrap()
        .as_array()
        .unwrap();
    assert!(!allowed.contains(&serde_json::Value::String("localhost".to_string())));
    assert!(!allowed.contains(&serde_json::Value::String("127.0.0.1".to_string())));
}
