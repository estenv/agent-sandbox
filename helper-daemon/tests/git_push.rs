mod common;
use common::*;

fn test_git_push_rejects_main_or_master() {
    let dir = tempfile::TempDir::new().unwrap();
    let mut child = start_daemon(dir.path());
    let sock = dir.path().join("daemon.sock");

    let (_, working) = init_bare_and_working(dir.path());
    commit_and_push(&working, "README", b"data", "main");

    let response = send_request(&sock, &format!("git-push {}", working.display()));
    let v: serde_json::Value = serde_json::from_str(&response).unwrap();
    assert!(
        !v["ok"].as_bool().unwrap(),
        "push to main should be rejected, got: {response}"
    );
    assert!(
        v["error"].as_str().unwrap().contains("protected branch"),
        "expected protected branch error, got: {response}"
    );

    let _ = child.kill();
    let _ = child.wait();
}

fn test_git_push_feature_branch_succeeds() {
    let dir = tempfile::TempDir::new().unwrap();
    let mut child = start_daemon(dir.path());
    let sock = dir.path().join("daemon.sock");

    let (_, working) = init_bare_and_working(dir.path());
    commit_and_push(&working, "README", b"base", "main");

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

    let response = send_request(&sock, &format!("git-push {}", working.display()));
    let v: serde_json::Value = serde_json::from_str(&response).unwrap();
    assert!(
        v["ok"].as_bool().unwrap(),
        "push feature branch should succeed, got: {response}"
    );
    assert_eq!(v["exit_code"].as_i64(), Some(0));

    let _ = child.kill();
    let _ = child.wait();
}

#[test]
fn sealed_git_push_scenarios() {
    test_git_push_rejects_main_or_master();
    test_git_push_feature_branch_succeeds();
}
