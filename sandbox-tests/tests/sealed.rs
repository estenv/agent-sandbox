use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

fn agent_sandbox_binary() -> PathBuf {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let profile = if cfg!(debug_assertions) {
        "debug"
    } else {
        "release"
    };
    manifest
        .parent()
        .unwrap()
        .join("target")
        .join(profile)
        .join("agent-sandbox")
}

fn find_srt() -> Option<PathBuf> {
    std::env::var_os("PATH").and_then(|p| {
        std::env::split_paths(&p)
            .map(|d| d.join("srt"))
            .find(|c| c.is_file())
    })
}

fn require_srt() {
    find_srt().unwrap_or_else(|| {
        panic!(
            "srt not found on PATH — these sealed integration tests require it. \
             Install from https://github.com/anthropic-experimental/sandbox-runtime"
        )
    });
}

fn run_sandbox(
    workspace: &Path,
    projects_root: &Path,
    args: &[&str],
    envs: &[(&str, &str)],
    timeout: Duration,
) -> Output {
    let mut cmd = Command::new(agent_sandbox_binary());
    cmd.arg("run")
        .arg(format!("--workspace={}", workspace.display()))
        .arg(format!("--projects-root={}", projects_root.display()))
        .arg("--")
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (key, val) in envs {
        cmd.env(key, val);
    }

    let mut child = cmd.spawn().expect("failed to spawn agent-sandbox");
    let start = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(s)) => break s,
            Ok(None) => {
                if start.elapsed() > timeout {
                    child.kill().ok();
                    let _ = child.wait();
                    panic!("test timed out after {timeout:.1?}");
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(e) => panic!("failed to wait for agent-sandbox: {e}"),
        }
    };
    let stdout = child
        .stdout
        .take()
        .map(|mut o| {
            let mut b = Vec::new();
            o.read_to_end(&mut b).ok();
            b
        })
        .unwrap_or_default();
    let stderr = child
        .stderr
        .take()
        .map(|mut e| {
            let mut b = Vec::new();
            e.read_to_end(&mut b).ok();
            b
        })
        .unwrap_or_default();
    Output {
        status,
        stdout,
        stderr,
    }
}

fn test_root(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("as-sealed-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[ignore]
#[test]
fn sealed_git_operations() {
    require_srt();
    direct_git_pull_fails_inside_sealed_sandbox();
    git_pull_via_daemon_succeeds_inside_sealed_sandbox();
}

fn direct_git_pull_fails_inside_sealed_sandbox() {
    let root = test_root("direct-fail");
    let projects = root.join("projects");
    let workspace = root.join("workspace");
    std::fs::create_dir_all(&projects).unwrap();

    Command::new("git")
        .args(["init"])
        .arg(&projects)
        .status()
        .unwrap();
    Command::new("git")
        .args(["-C", projects.to_str().unwrap(), "remote", "add", "origin"])
        .arg("https://github.com/anthropic-experimental/sandbox-runtime.git")
        .status()
        .unwrap();

    let output = run_sandbox(
        &workspace,
        &projects,
        &["git", "pull"],
        &[
            ("GIT_TERMINAL_TIMEOUT", "10"),
            ("GIT_TERMINAL_PROMPT", "0"),
            ("GIT_ASKPASS", ""),
        ],
        Duration::from_secs(30),
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{}\n{}", String::from_utf8_lossy(&output.stdout), &stderr);

    assert!(
        !output.status.success(),
        "git pull should have failed inside the sandbox. output:\n{combined}"
    );
    assert!(
        stderr.contains("Forbidden")
            || stderr.contains("CONNECT tunnel failed")
            || stderr.contains("Could not resolve host")
            || stderr.contains("Cannot assign requested address")
            || stderr.contains("failure in name resolution")
            || combined.contains("Could not read from remote repository"),
        "expected a network-blocked error. stderr:\n{stderr}"
    );
    let _ = std::fs::remove_dir_all(&root);
}

fn git_pull_via_daemon_succeeds_inside_sealed_sandbox() {
    let root = test_root("daemon-ok");
    let projects = root.join("projects");
    let workspace = root.join("workspace");
    std::fs::create_dir_all(&projects).unwrap();

    let bare = projects.join("bare.git");
    let working = projects.join("working");
    let updater = projects.join("updater");

    Command::new("git")
        .args(["init", "--bare"])
        .arg(&bare)
        .status()
        .unwrap();
    Command::new("git")
        .args(["clone", bare.to_str().unwrap()])
        .arg(&working)
        .status()
        .unwrap();

    std::fs::write(working.join("README"), b"hello").unwrap();
    Command::new("git")
        .args(["-C", working.to_str().unwrap(), "add", "README"])
        .status()
        .unwrap();
    Command::new("git")
        .args(["-C", working.to_str().unwrap(), "commit", "-m", "initial"])
        .status()
        .unwrap();
    Command::new("git")
        .args(["-C", working.to_str().unwrap(), "push", "origin", "master"])
        .status()
        .unwrap();

    Command::new("git")
        .args(["clone", bare.to_str().unwrap()])
        .arg(&updater)
        .status()
        .unwrap();
    std::fs::write(updater.join("NEW"), b"world").unwrap();
    Command::new("git")
        .args(["-C", updater.to_str().unwrap(), "add", "NEW"])
        .status()
        .unwrap();
    Command::new("git")
        .args(["-C", updater.to_str().unwrap(), "commit", "-m", "second"])
        .status()
        .unwrap();
    Command::new("git")
        .args(["-C", updater.to_str().unwrap(), "push", "origin", "master"])
        .status()
        .unwrap();

    let output = run_sandbox(
        &workspace,
        &projects,
        &[
            "agent-sandbox-helper",
            "git-pull",
            working.to_str().unwrap(),
        ],
        &[],
        Duration::from_secs(30),
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let v: serde_json::Value =
        serde_json::from_str(stdout.trim()).expect("response should be valid JSON");
    assert!(
        v["ok"].as_bool().unwrap_or(false),
        "daemon git-pull should succeed. response:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        working.join("NEW").exists(),
        "pulled file NEW should exist after git-pull"
    );
    let _ = std::fs::remove_dir_all(&root);
}
