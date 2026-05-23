use std::path::PathBuf;

#[test]
fn test_prepare_settings_expands_tilde() {
    use std::io::Write;

    let dir = std::env::temp_dir().join("agent-sandbox-test-prepare");
    let _ = std::fs::create_dir_all(&dir);
    let settings_path = dir.join("settings.json");
    let mut f = std::fs::File::create(&settings_path).unwrap();
    f.write_all(agent_sandbox::policy::DEFAULT_SETTINGS_JSON.as_bytes())
        .unwrap();
    f.flush().unwrap();

    let projects_root = PathBuf::from("/tmp/test-projects");
    let result =
        agent_sandbox::policy::prepare_settings(&settings_path, &projects_root).unwrap();

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

    // Verify deniedDomains is present
    assert!(parsed.pointer("/network/deniedDomains").is_some());

    let _ = std::fs::remove_dir_all(&dir);
}
