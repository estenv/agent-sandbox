mod common;
use common::*;

fn test_git_pull_in_git_repo() {
    let dir = tempfile::TempDir::new().unwrap();
    let mut child = start_daemon(dir.path());
    let sock = dir.path().join("daemon.sock");

    let (_, working) = init_bare_and_working(dir.path());

    std::fs::write(working.join("README"), b"hello").unwrap();
    git()
        .args(["add", "README"])
        .current_dir(&working)
        .status()
        .expect("git add");
    git()
        .args(["commit", "-m", "initial"])
        .current_dir(&working)
        .status()
        .expect("git commit");
    git()
        .args(["push", "origin", "master"])
        .current_dir(&working)
        .status()
        .expect("git push");

    let updater = dir.path().join("updater");
    git()
        .args(["clone", dir.path().join("bare.git").to_str().unwrap()])
        .arg(&updater)
        .status()
        .expect("clone for updater");
    std::fs::write(updater.join("NEW"), b"world").unwrap();
    git()
        .args(["add", "NEW"])
        .current_dir(&updater)
        .status()
        .expect("updater add");
    git()
        .args(["commit", "-m", "second"])
        .current_dir(&updater)
        .status()
        .expect("updater commit");
    git()
        .args(["push", "origin", "master"])
        .current_dir(&updater)
        .status()
        .expect("updater push");

    let response = send_request(&sock, &format!("git-pull {}", working.display()));
    let v: serde_json::Value = serde_json::from_str(&response).unwrap();
    assert!(
        v["ok"].as_bool().unwrap(),
        "git-pull should succeed, got: {response}"
    );
    assert_eq!(v["exit_code"].as_i64(), Some(0));

    assert!(working.join("NEW").exists(), "pulled file should exist");

    let _ = child.kill();
    let _ = child.wait();
}

#[test]
fn sealed_git_pull_scenarios() {
    test_git_pull_in_git_repo();
}
