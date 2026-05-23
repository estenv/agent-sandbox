use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

/// Find the compiled agent-sandbox binary.
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

/// Find srt on PATH.
fn find_srt() -> Option<PathBuf> {
    std::env::var_os("PATH").as_ref().and_then(|path| {
        std::env::split_paths(path).find_map(|dir| {
            let candidate = dir.join("srt");
            candidate.is_file().then_some(candidate)
        })
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

/// Run agent-sandbox with the given args and return stdout + stderr.
/// `envs` are environment variables set on the sandboxed command (inherited through srt/bubblewrap).
fn run_sandbox(
    workspace: &Path,
    projects_root: &Path,
    args: &[&str],
    envs: &[(&str, &str)],
    timeout: Duration,
) -> Output {
    let bin = agent_sandbox_binary();
    let ws_flag = format!("--workspace={}", workspace.display());
    let pr_flag = format!("--projects-root={}", projects_root.display());

    let mut cmd = Command::new(&bin);
    cmd.arg("run")
        .arg(&ws_flag)
        .arg(&pr_flag)
        .arg("--")
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (key, val) in envs {
        cmd.env(key, val);
    }

    eprintln!("[sandbox-tests] running: {bin:?} run {ws_flag} {pr_flag} -- {args:?}");

    let now = Instant::now();
    let mut child = cmd.spawn().expect("failed to spawn agent-sandbox");

    // Take pipes and read in threads to avoid pipe buffer deadlocks
    let mut child_stdout = child.stdout.take().unwrap();
    let mut child_stderr = child.stderr.take().unwrap();
    let stdout_handle = std::thread::spawn(move || {
        let mut buf = Vec::new();
        child_stdout.read_to_end(&mut buf).ok();
        buf
    });
    let stderr_handle = std::thread::spawn(move || {
        let mut buf = Vec::new();
        child_stderr.read_to_end(&mut buf).ok();
        buf
    });

    // Poll for completion with actual timeout enforcement
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {
                if now.elapsed() > timeout {
                    child.kill().ok();
                    let _ = child.wait(); // reap to avoid zombie
                    panic!("test timed out after {timeout:.1?}");
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(e) => panic!("failed to wait for agent-sandbox: {e}"),
        }
    };

    let stdout = stdout_handle.join().unwrap();
    let stderr = stderr_handle.join().unwrap();

    let elapsed = now.elapsed();
    eprintln!(
        "[sandbox-tests] completed in {elapsed:.1?} (status={status})"
    );

    Output {
        status,
        stdout,
        stderr,
    }
}

/// Create a temp root directory for a sealed test.
fn test_root(name: &str) -> PathBuf {
    let pid = std::process::id();
    let dir = std::env::temp_dir().join(format!("as-sealed-{name}-{pid}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

// ----------------------------------------------------------------
// All sealed tests run sequentially inside a single test function
// so that concurrent sandbox sessions don't contend for resources.
// ----------------------------------------------------------------

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

    // Make the projects root itself a git repo so that `git pull`
    // (which runs inside the sandbox with CWD = projects root) finds .git.
    Command::new("git")
        .args(["init"])
        .arg(&projects)
        .status()
        .expect("git init");
    Command::new("git")
        .args(["-C", projects.to_str().unwrap(), "remote", "add", "origin"])
        .arg("https://github.com/anthropic-experimental/sandbox-runtime.git")
        .status()
        .expect("git remote add");

    // git pull inside the sealed sandbox — must fail (network blocked)
    // Multiple git env vars ensure it fails fast regardless of how the
    // sandbox's network block manifests (silent drops vs immediate reject)
    // and regardless of the parent shell's terminal/pipe behavior.
    let output = run_sandbox(
        &workspace,
        &projects,
        &["git", "pull"],
        &[
            ("GIT_TERMINAL_TIMEOUT", "10"),
            ("GIT_TERMINAL_PROMPT", "0"),
            ("GIT_ASKPASS", ""),
            ("SSH_ASKPASS", ""),
            ("GIT_PAGER", "cat"),
            ("PAGER", "cat"),
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

    // Cleanup
    let _ = std::fs::remove_dir_all(&root);
}

fn git_pull_via_daemon_succeeds_inside_sealed_sandbox() {
    let root = test_root("daemon-ok");
    let projects = root.join("projects");
    let workspace = root.join("workspace");
    std::fs::create_dir_all(&projects).unwrap();

    // Create a bare remote repo
    let bare = projects.join("bare.git");
    Command::new("git")
        .args(["init", "--bare"])
        .arg(&bare)
        .status()
        .expect("git init --bare");

    // Clone a working copy
    let working = projects.join("working");
    Command::new("git")
        .args(["clone", bare.to_str().unwrap()])
        .arg(&working)
        .status()
        .expect("git clone");

    // Make initial commit and push
    std::fs::write(working.join("README"), b"hello").unwrap();
    Command::new("git")
        .args(["-C", working.to_str().unwrap(), "add", "README"])
        .status()
        .expect("git add");
    Command::new("git")
        .args(["-C", working.to_str().unwrap(), "commit", "-m", "initial"])
        .status()
        .expect("git commit");
    Command::new("git")
        .args(["-C", working.to_str().unwrap(), "push", "origin", "master"])
        .status()
        .expect("git push");

    // Push a second commit from another clone (simulate upstream changes)
    let updater = projects.join("updater");
    Command::new("git")
        .args(["clone", bare.to_str().unwrap()])
        .arg(&updater)
        .status()
        .expect("git clone updater");
    std::fs::write(updater.join("NEW"), b"world").unwrap();
    Command::new("git")
        .args(["-C", updater.to_str().unwrap(), "add", "NEW"])
        .status()
        .expect("updater git add");
    Command::new("git")
        .args(["-C", updater.to_str().unwrap(), "commit", "-m", "second"])
        .status()
        .expect("updater git commit");
    Command::new("git")
        .args(["-C", updater.to_str().unwrap(), "push", "origin", "master"])
        .status()
        .expect("updater git push");

    // Run agent-sandbox-helper git-pull inside the sandbox
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

    // Parse the JSON response from the daemon
    let v: serde_json::Value =
        serde_json::from_str(stdout.trim()).expect("response should be valid JSON");

    assert!(
        v["ok"].as_bool().unwrap_or(false),
        "daemon git-pull should succeed. response:\n{stdout}\nstderr:\n{stderr}"
    );

    // Verify the pulled file actually exists
    assert!(
        working.join("NEW").exists(),
        "pulled file NEW should exist after git-pull"
    );

    // Cleanup
    let _ = std::fs::remove_dir_all(&root);
}
