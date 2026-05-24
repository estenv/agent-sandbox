use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

pub fn git() -> Command {
    let mut cmd = Command::new("git");
    cmd.stdout(Stdio::null())
        .stderr(Stdio::null())
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_ASKPASS", "");
    cmd
}

pub fn start_daemon(dir: &Path) -> Child {
    start_daemon_with_root(dir, None)
}

pub fn start_daemon_with_root(dir: &Path, projects_root: Option<&Path>) -> Child {
    let sock = dir.join("daemon.sock");

    let mut cmd = Command::new(daemon_binary());
    cmd.arg("--socket-path")
        .arg(&sock)
        .stdin(Stdio::null())
        .stderr(Stdio::piped())
        .stdout(Stdio::null());
    if let Some(root) = projects_root {
        cmd.arg("--projects-root").arg(root);
    }
    let mut child = cmd.spawn().expect("failed to start daemon");

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

pub fn daemon_binary() -> PathBuf {
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

pub fn send_request(sock: &Path, request: &str) -> String {
    let mut conn = UnixStream::connect(sock).expect("connect to daemon");
    writeln!(conn, "{request}").expect("write request");
    let mut response = Vec::new();
    conn.read_to_end(&mut response).expect("read response");
    String::from_utf8_lossy(&response).to_string()
}

pub fn init_bare_and_working(dir: &Path) -> (PathBuf, PathBuf) {
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
    (bare, working)
}

#[allow(dead_code)]
pub fn commit_and_push(working: &Path, filename: &str, content: &[u8], branch: &str) {
    std::fs::write(working.join(filename), content).unwrap();
    git()
        .args(["add", filename])
        .current_dir(working)
        .status()
        .expect("add");
    git()
        .args(["commit", "-m", "initial"])
        .current_dir(working)
        .status()
        .expect("commit");
    git()
        .args(["push", "-u", "origin", branch])
        .current_dir(working)
        .status()
        .expect("push");
}
