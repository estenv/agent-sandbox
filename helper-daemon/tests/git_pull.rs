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

fn git() -> Command {
    let mut cmd = Command::new("git");
    cmd.stdout(Stdio::null())
        .stderr(Stdio::null())
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_ASKPASS", "");
    cmd
}

/// Start the daemon on a temp socket and return (guard, socket_path).
fn start_daemon() -> (DaemonGuard, PathBuf) {
    let dir = unique_dir("test");
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

    // Drain daemon stderr in a background thread to prevent pipe-buffer deadlock
    let mut daemon_stderr = child.stderr.take().unwrap();
    std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = daemon_stderr.read_to_end(&mut buf);
    });

    // Poll for socket readiness
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

#[test]
fn sealed_git_pull_scenarios() {
    test_git_pull_in_git_repo();
    test_git_pull_outside_projects_root_rejected();
}

fn test_git_pull_in_git_repo() {
    let (_guard, sock) = start_daemon();

    let dir = unique_dir("test-repo");
    std::fs::create_dir_all(&dir).unwrap();

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

    let updater = dir.join("updater");
    git()
        .args(["clone", bare.to_str().unwrap()])
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
    let _ = std::fs::remove_dir_all(&dir);
}

fn test_git_pull_outside_projects_root_rejected() {
    let dir = unique_dir("test-rooted");
    std::fs::create_dir_all(&dir).unwrap();
    let sock = dir.join("daemon.sock");

    let root = dir.join("allowed");
    std::fs::create_dir_all(&root).unwrap();

    let mut child = Command::new(daemon_binary())
        .arg("--socket-path")
        .arg(&sock)
        .arg("--projects-root")
        .arg(&root)
        .stdin(Stdio::null())
        .stderr(Stdio::piped())
        .stdout(Stdio::null())
        .spawn()
        .expect("start daemon");

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
            panic!("daemon did not become reachable");
        }
        std::thread::sleep(Duration::from_millis(100));
    }

    let response = send_request(&sock, &format!("git-pull /tmp"));
    let v: serde_json::Value = serde_json::from_str(&response).unwrap();
    assert!(!v["ok"].as_bool().unwrap());
    assert!(v["error"]
        .as_str()
        .unwrap()
        .contains("outside allowed projects root"));

    let _ = child.kill();
    let _ = child.wait();
    let _ = std::fs::remove_dir_all(&dir);
}
