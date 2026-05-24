use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

fn git() -> Command {
    let mut cmd = Command::new("git");
    cmd.stdout(Stdio::null())
        .stderr(Stdio::null())
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_ASKPASS", "");
    cmd
}

fn start_daemon(dir: &std::path::Path) -> Child {
    let sock = dir.join("daemon.sock");

    let mut child = Command::new(daemon_binary())
        .arg("--socket-path")
        .arg(&sock)
        .stdin(Stdio::null())
        .stderr(Stdio::piped())
        .stdout(Stdio::null())
        .spawn()
        .expect("failed to start daemon");

    let mut daemon_stderr = child.stderr.take().unwrap();
    std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = daemon_stderr.read_to_end(&mut buf);
    });

    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if sock.exists() {
            if let Ok(mut conn) = UnixStream::connect(&sock) {
                let _ = writeln!(conn, "healthz");
                break;
            }
        }
        if Instant::now() > deadline {
            panic!("daemon did not become reachable within 5s");
        }
        std::thread::sleep(Duration::from_millis(100));
    }

    child
}

fn daemon_binary() -> PathBuf {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let profile = if cfg!(debug_assertions) {
        "debug"
    } else {
        "release"
    };
    path.push("..");
    path.push("target");
    path.push(profile);
    path.push("agent-sandbox-helper-daemon");
    path
}

fn send_request(sock: &PathBuf, request: &str) -> String {
    let mut conn = UnixStream::connect(sock).expect("connect to daemon");
    writeln!(conn, "{request}").expect("write request");
    let mut response = Vec::new();
    conn.read_to_end(&mut response).expect("read response");
    String::from_utf8_lossy(&response).to_string()
}

fn git_init_bare_working(dir: &std::path::Path) -> PathBuf {
    let bare = dir.join("bare.git");
    git()
        .args(["init", "--bare"])
        .arg(&bare)
        .status()
        .expect("init bare repo");

    let working = dir.join("working");
    git()
        .args(["clone", bare.to_str().unwrap()])
        .arg(&working)
        .status()
        .expect("clone bare repo");

    std::fs::write(working.join("README"), b"base").unwrap();
    git()
        .args(["add", "README"])
        .current_dir(&working)
        .status()
        .expect("add");
    git()
        .args(["commit", "-m", "initial"])
        .current_dir(&working)
        .status()
        .expect("commit");
    git()
        .args(["push", "-u", "origin", "master"])
        .current_dir(&working)
        .status()
        .expect("initial push");

    working
}

fn set_remote_to_ado(working: &std::path::Path, org: &str, project: &str, repo: &str) {
    let url = format!("https://dev.azure.com/{org}/{project}/_git/{repo}");
    git()
        .args(["remote", "set-url", "origin", &url])
        .current_dir(working)
        .status()
        .expect("set remote");
}

#[test]
fn sealed_pr_create_scenarios() {
    test_pr_create_invalid_json();
    test_pr_create_relative_path();
    test_pr_create_no_remote();
    test_pr_create_ado_remote_no_az();
}

fn test_pr_create_invalid_json() {
    let dir = tempfile::TempDir::new().unwrap();
    let mut child = start_daemon(dir.path());
    let sock = dir.path().join("daemon.sock");

    let response = send_request(&sock, "pr-create not-json");
    let v: serde_json::Value = serde_json::from_str(&response).unwrap();
    assert!(!v["ok"].as_bool().unwrap());
    assert!(v["error"]
        .as_str()
        .unwrap()
        .contains("invalid pr-create JSON"));

    let _ = child.kill();
    let _ = child.wait();
}

fn test_pr_create_relative_path() {
    let dir = tempfile::TempDir::new().unwrap();
    let mut child = start_daemon(dir.path());
    let sock = dir.path().join("daemon.sock");

    let json = serde_json::json!({
        "path": "relative/path",
        "title": "Test",
        "source": "feature",
    });
    let response = send_request(&sock, &format!("pr-create {json}"));
    let v: serde_json::Value = serde_json::from_str(&response).unwrap();
    assert!(!v["ok"].as_bool().unwrap());
    assert_eq!(v["error"].as_str().unwrap(), "path must be absolute");

    let _ = child.kill();
    let _ = child.wait();
}

fn test_pr_create_no_remote() {
    let dir = tempfile::TempDir::new().unwrap();
    let mut child = start_daemon(dir.path());
    let sock = dir.path().join("daemon.sock");

    let working = git_init_bare_working(dir.path());

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

    let working = git_init_bare_working(dir.path());

    set_remote_to_ado(&working, "myorg", "myproject", "myrepo");

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
