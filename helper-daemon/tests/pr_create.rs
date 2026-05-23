use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

fn unique_dir(label: &str) -> PathBuf {
    let n = TEST_COUNTER.fetch_add(1, Ordering::SeqCst);
    std::env::temp_dir().join(format!("as-{label}-{n}"))
}

fn start_daemon() -> (DaemonGuard, PathBuf) {
    let dir = unique_dir("pr-test");
    std::fs::create_dir_all(&dir).unwrap();
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

    (
        DaemonGuard {
            child,
            dir: dir.clone(),
        },
        sock,
    )
}

struct DaemonGuard {
    child: Child,
    dir: PathBuf,
}

impl Drop for DaemonGuard {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_dir_all(&self.dir);
    }
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

fn git_init_bare_working() -> (PathBuf, PathBuf) {
    let dir = unique_dir("pr-scenario");
    std::fs::create_dir_all(&dir).unwrap();

    let bare = dir.join("bare.git");
    Command::new("git")
        .args(["init", "--bare"])
        .arg(&bare)
        .status()
        .expect("init bare repo");

    let working = dir.join("working");
    Command::new("git")
        .args(["clone", bare.to_str().unwrap()])
        .arg(&working)
        .status()
        .expect("clone bare repo");

    std::fs::write(working.join("README"), b"base").unwrap();
    Command::new("git")
        .args(["add", "README"])
        .current_dir(&working)
        .status()
        .expect("add");
    Command::new("git")
        .args(["commit", "-m", "initial"])
        .current_dir(&working)
        .status()
        .expect("commit");
    Command::new("git")
        .args(["push", "-u", "origin", "master"])
        .current_dir(&working)
        .status()
        .expect("initial push");

    (dir, working)
}

fn set_remote_to_ado(working: &PathBuf, org: &str, project: &str, repo: &str) {
    let url = format!("https://dev.azure.com/{org}/{project}/_git/{repo}");
    Command::new("git")
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
    let (_guard, sock) = start_daemon();
    let response = send_request(&sock, "pr-create not-json");
    let v: serde_json::Value = serde_json::from_str(&response).unwrap();
    assert!(!v["ok"].as_bool().unwrap());
    assert!(v["error"]
        .as_str()
        .unwrap()
        .contains("invalid pr-create JSON"));
}

fn test_pr_create_relative_path() {
    let (_guard, sock) = start_daemon();
    let json = serde_json::json!({
        "path": "relative/path",
        "title": "Test",
        "source": "feature",
    });
    let response = send_request(&sock, &format!("pr-create {json}"));
    let v: serde_json::Value = serde_json::from_str(&response).unwrap();
    assert!(!v["ok"].as_bool().unwrap());
    assert_eq!(v["error"].as_str().unwrap(), "path must be absolute");
}

fn test_pr_create_no_remote() {
    let (_guard, sock) = start_daemon();
    let (_dir, working) = git_init_bare_working();

    // Remove remote so git remote get-url origin fails
    Command::new("git")
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

    let _ = std::fs::remove_dir_all(&_dir);
}

fn test_pr_create_ado_remote_no_az() {
    let (_guard, sock) = start_daemon();
    let (_dir, working) = git_init_bare_working();

    set_remote_to_ado(&working, "myorg", "myproject", "myrepo");

    // Create a feature branch
    Command::new("git")
        .args(["checkout", "-b", "feature-x"])
        .current_dir(&working)
        .status()
        .expect("checkout feature branch");

    std::fs::write(working.join("FEATURE"), b"new stuff").unwrap();
    Command::new("git")
        .args(["add", "FEATURE"])
        .current_dir(&working)
        .status()
        .expect("add feature");
    Command::new("git")
        .args(["commit", "-m", "feature work"])
        .current_dir(&working)
        .status()
        .expect("commit feature");
    Command::new("git")
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

    // az may not be installed or authenticated — expect a clean error, not a crash
    assert!(!v["ok"].as_bool().unwrap());
    let err = v["error"].as_str().unwrap();
    assert!(
        err.contains("failed to execute az") || err.contains("az repos"),
        "expected az-related error, got: {err}"
    );

    let _ = std::fs::remove_dir_all(&_dir);
}
