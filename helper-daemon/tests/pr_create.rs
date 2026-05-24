mod common;
use common::*;

fn test_pr_create_no_remote() {
    let dir = tempfile::TempDir::new().unwrap();
    let mut child = start_daemon(dir.path());
    let sock = dir.path().join("daemon.sock");

    let (_, working) = init_bare_and_working(dir.path());
    commit_and_push(&working, "README", b"base", "master");

    git()
        .args(["remote", "remove", "origin"])
        .current_dir(&working)
        .status()
        .expect("remove remote");

    let json = serde_json::json!({
        "path": working.to_str().unwrap(),
        "title": "Test",
        "source": "feature",
    });
    let response = send_request(&sock, &format!("pr-create {json}"));
    let v: serde_json::Value = serde_json::from_str(&response).unwrap();
    assert!(!v["ok"].as_bool().unwrap());
    let err = v["error"].as_str().unwrap();
    assert!(
        err.contains("remote 'origin'"),
        "expected no-remote error, got: {err}"
    );

    let _ = child.kill();
    let _ = child.wait();
}

fn test_pr_create_ado_remote_no_az() {
    let dir = tempfile::TempDir::new().unwrap();
    let mut child = start_daemon(dir.path());
    let sock = dir.path().join("daemon.sock");

    let (_, working) = init_bare_and_working(dir.path());
    commit_and_push(&working, "README", b"base", "master");

    let url = "https://dev.azure.com/myorg/myproject/_git/myrepo";
    git()
        .args(["remote", "set-url", "origin", url])
        .current_dir(&working)
        .status()
        .expect("set remote");

    git()
        .args(["checkout", "-b", "feature-x"])
        .current_dir(&working)
        .status()
        .expect("checkout feature branch");

    std::fs::write(working.join("FEATURE"), b"new stuff").unwrap();
    git()
        .args(["add", "FEATURE"])
        .current_dir(&working)
        .status()
        .expect("add feature");
    git()
        .args(["commit", "-m", "feature work"])
        .current_dir(&working)
        .status()
        .expect("commit feature");
    git()
        .args(["push", "-u", "origin", "feature-x"])
        .current_dir(&working)
        .status()
        .expect("setup upstream");

    let json = serde_json::json!({
        "path": working.to_str().unwrap(),
        "title": "Test PR from daemon",
        "source": "feature-x",
        "target": "master",
        "description": "Automated PR via agent-sandbox",
    });
    let response = send_request(&sock, &format!("pr-create {json}"));
    let v: serde_json::Value = serde_json::from_str(&response).unwrap();

    assert!(!v["ok"].as_bool().unwrap());
    let err = v["error"].as_str().unwrap();
    assert!(
        err.contains("failed to execute az") || err.contains("az repos"),
        "expected az-related error, got: {err}"
    );

    let _ = child.kill();
    let _ = child.wait();
}

#[test]
fn sealed_pr_create_scenarios() {
    test_pr_create_no_remote();
    test_pr_create_ado_remote_no_az();
}
