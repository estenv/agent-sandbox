use clap::Parser;
use std::fs;
use std::io::{Read, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

#[derive(Debug, Parser)]
#[command(name = "agent-sandbox-helper-daemon")]
#[command(about = "Host helper daemon for agent-sandbox — listens on a Unix socket")]
#[command(version)]
struct Cli {
    /// Path to the Unix domain socket.
    #[arg(long, default_value = "~/.agent-sandbox/daemon.sock")]
    socket_path: String,

    /// Restrict file operations to this root directory.
    #[arg(long)]
    projects_root: Option<String>,
}

fn main() -> ExitCode {
    match real_main() {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("agent-sandbox-helper-daemon: {err}");
            ExitCode::from(1)
        }
    }
}

fn real_main() -> std::io::Result<()> {
    let cli = Cli::parse();
    let socket_path = resolve_path(&cli.socket_path);
    let projects_root = cli.projects_root.as_deref().map(resolve_path);

    let _ = fs::remove_file(&socket_path);

    if let Some(parent) = socket_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let listener = UnixListener::bind(&socket_path)?;
    eprintln!(
        "agent-sandbox-helper-daemon listening on {}",
        socket_path.display()
    );

    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                if let Err(err) = handle_connection(stream, projects_root.as_deref()) {
                    eprintln!("request failed: {err}");
                }
            }
            Err(err) => eprintln!("accept failed: {err}"),
        }
    }

    Ok(())
}

fn handle_connection(mut stream: UnixStream, projects_root: Option<&Path>) -> std::io::Result<()> {
    let mut buffer = [0_u8; 4096];
    let n = stream.read(&mut buffer)?;
    let request = String::from_utf8_lossy(&buffer[..n]);
    let line = request.lines().next().unwrap_or_default().trim().to_owned();
    let body = handle_request(&line, projects_root);
    stream.write_all(body.as_bytes())?;
    stream.flush()
}

fn ok_response(data: serde_json::Value) -> String {
    let mut resp = serde_json::json!({"ok": true});
    if let serde_json::Value::Object(ref mut obj) = resp {
        if let serde_json::Value::Object(extra) = data {
            obj.extend(extra);
        }
    }
    resp.to_string()
}

fn err_response(error: impl Into<String>) -> String {
    serde_json::json!({"ok": false, "error": error.into()}).to_string()
}

fn handle_request(line: &str, projects_root: Option<&Path>) -> String {
    let line = line.trim_start_matches('/').trim();
    let mut parts = line.splitn(2, ' ');
    let action = parts.next().unwrap_or("");
    let arg = parts.next().unwrap_or("").trim();

    match action {
        "healthz" => ok_response(serde_json::json!({"service": "agent-sandbox-helper-daemon"})),
        "test" => ok_response(serde_json::json!({"message": "helper daemon connectivity works"})),
        "git-pull" => {
            if arg.is_empty() {
                return err_response("usage: git-pull <absolute-path>");
            }
            let cwd = Path::new(arg);
            if !cwd.is_absolute() {
                return err_response("path must be absolute");
            }
            if let Some(root) = projects_root {
                if !cwd.starts_with(root) {
                    return err_response(format!(
                        "path is outside allowed projects root: {}",
                        cwd.display()
                    ));
                }
            }
            git_pull(cwd)
        }
        "git-push" => {
            if arg.is_empty() {
                return err_response("usage: git-push <absolute-path>");
            }
            let cwd = Path::new(arg);
            if !cwd.is_absolute() {
                return err_response("path must be absolute");
            }
            if let Some(root) = projects_root {
                if !cwd.starts_with(root) {
                    return err_response(format!(
                        "path is outside allowed projects root: {}",
                        cwd.display()
                    ));
                }
            }
            git_push(cwd)
        }
        "pr-create" => {
            let params = match parse_pr_params(arg) {
                Ok(p) => p,
                Err(e) => return err_response(e),
            };
            pr_create(&params, projects_root)
        }
        _ => err_response("not found"),
    }
}

fn git_pull(cwd: &Path) -> String {
    match Command::new("git").args(["pull"]).current_dir(cwd).output() {
        Ok(output) => {
            let exit_code = output.status.code().unwrap_or(-1);
            serde_json::json!({
                "ok": output.status.success(),
                "exit_code": exit_code,
                "stdout": String::from_utf8_lossy(&output.stdout),
                "stderr": String::from_utf8_lossy(&output.stderr),
            })
            .to_string()
        }
        Err(e) => err_response(format!("failed to execute git: {e}")),
    }
}

fn git_push(cwd: &Path) -> String {
    let branch = match current_branch(cwd) {
        Ok(b) => b,
        Err(e) => return err_response(e),
    };

    if is_protected_branch(&branch) {
        return err_response(format!(
            "pushing to protected branch '{branch}' is not allowed (main/master are protected)"
        ));
    }

    match Command::new("git").args(["push"]).current_dir(cwd).output() {
        Ok(output) => {
            let exit_code = output.status.code().unwrap_or(-1);
            serde_json::json!({
                "ok": output.status.success(),
                "exit_code": exit_code,
                "stdout": String::from_utf8_lossy(&output.stdout),
                "stderr": String::from_utf8_lossy(&output.stderr),
            })
            .to_string()
        }
        Err(e) => err_response(format!("failed to execute git: {e}")),
    }
}

fn current_branch(cwd: &Path) -> Result<String, String> {
    let output = Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .current_dir(cwd)
        .output()
        .map_err(|e| format!("failed to execute git: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "failed to determine current branch: {}",
            stderr.trim()
        ));
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn is_protected_branch(branch: &str) -> bool {
    matches!(branch, "main" | "master")
}

// --- PR Creation ---

#[derive(Debug)]
struct PrParams {
    path: String,
    title: String,
    source: String,
    target: Option<String>,
    description: Option<String>,
}

fn parse_pr_params(arg: &str) -> Result<PrParams, String> {
    let v: serde_json::Value =
        serde_json::from_str(arg).map_err(|e| format!("invalid pr-create JSON: {e}"))?;

    let obj = v
        .as_object()
        .ok_or("pr-create params must be a JSON object")?;

    let extract = |key: &str| -> Result<String, String> {
        obj.get(key)
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| format!("missing required field '{key}'"))
    };

    Ok(PrParams {
        path: extract("path")?,
        title: extract("title")?,
        source: extract("source")?,
        target: obj.get("target").and_then(|v| v.as_str()).map(String::from),
        description: obj
            .get("description")
            .and_then(|v| v.as_str())
            .map(String::from),
    })
}

enum GitProvider {
    AzureDevops {
        org: String,
        project: String,
        repo: String,
    },
}

fn pr_create(params: &PrParams, projects_root: Option<&Path>) -> String {
    let cwd = Path::new(&params.path);
    if !cwd.is_absolute() {
        return err_response("path must be absolute");
    }
    if let Some(root) = projects_root {
        if !cwd.starts_with(root) {
            return err_response(format!(
                "path is outside allowed projects root: {}",
                cwd.display()
            ));
        }
    }

    let provider = match detect_provider(cwd) {
        Ok(p) => p,
        Err(e) => return err_response(e),
    };

    match provider {
        GitProvider::AzureDevops { org, project, repo } => {
            create_ado_pr(cwd, &org, &project, &repo, params)
        }
    }
}

fn detect_provider(cwd: &Path) -> Result<GitProvider, String> {
    let output = Command::new("git")
        .args(["remote", "get-url", "origin"])
        .current_dir(cwd)
        .output()
        .map_err(|e| format!("failed to execute git: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("failed to get remote 'origin': {}", stderr.trim()));
    }

    let remote = String::from_utf8_lossy(&output.stdout).trim().to_string();

    if let Some(provider) = try_parse_ado_https(&remote) {
        return Ok(provider);
    }
    if let Some(provider) = try_parse_ado_ssh(&remote) {
        return Ok(provider);
    }

    Err(format!("unsupported git remote: {remote}"))
}

fn try_parse_ado_https(remote: &str) -> Option<GitProvider> {
    let remote = remote.strip_suffix(".git").unwrap_or(remote);
    let remote = remote.strip_prefix("https://")?;

    let path = if let Some(at) = remote.find('@') {
        &remote[at + 1..]
    } else {
        remote
    };

    let path = path.strip_prefix("dev.azure.com/")?;
    let segments: Vec<&str> = path.split('/').collect();

    if segments.len() >= 4 && segments[2] == "_git" {
        Some(GitProvider::AzureDevops {
            org: segments[0].to_string(),
            project: segments[1].to_string(),
            repo: segments[3].to_string(),
        })
    } else {
        None
    }
}

fn try_parse_ado_ssh(remote: &str) -> Option<GitProvider> {
    let remote = remote.strip_prefix("git@ssh.dev.azure.com:v3/")?;
    let segments: Vec<&str> = remote.split('/').collect();

    if segments.len() >= 3 {
        Some(GitProvider::AzureDevops {
            org: segments[0].to_string(),
            project: segments[1].to_string(),
            repo: segments[2].to_string(),
        })
    } else {
        None
    }
}

fn create_ado_pr(cwd: &Path, org: &str, project: &str, repo: &str, params: &PrParams) -> String {
    let mut cmd = Command::new("az");
    cmd.arg("repos")
        .arg("pull-request")
        .arg("create")
        .arg("--org")
        .arg(format!("https://dev.azure.com/{org}"))
        .arg("--project")
        .arg(project)
        .arg("--repository")
        .arg(repo)
        .arg("--source-branch")
        .arg(&params.source)
        .arg("--title")
        .arg(&params.title)
        .arg("--output")
        .arg("json")
        .current_dir(cwd);

    if let Some(target) = &params.target {
        cmd.arg("--target-branch").arg(target);
    }
    if let Some(desc) = &params.description {
        cmd.arg("--description").arg(desc);
    }

    match cmd.output() {
        Ok(output) => {
            let exit_code = output.status.code().unwrap_or(-1);
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                match serde_json::from_str::<serde_json::Value>(&stdout) {
                    Ok(json) => {
                        let url = json.get("url").and_then(|u| u.as_str()).unwrap_or("");
                        let pr_id = json
                            .get("pullRequestId")
                            .and_then(|v| v.as_i64())
                            .unwrap_or(0);
                        ok_response(serde_json::json!({
                            "provider": "azure-devops",
                            "pull_request_id": pr_id,
                            "url": url,
                        }))
                    }
                    Err(_) => ok_response(serde_json::json!({
                        "provider": "azure-devops",
                        "raw_output": stdout,
                    })),
                }
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr);
                err_response(format!(
                    "az repos pull-request create failed (exit {exit_code}): {}",
                    stderr.trim()
                ))
            }
        }
        Err(e) => err_response(format!("failed to execute az: {e}")),
    }
}

fn resolve_path(path: &str) -> PathBuf {
    if let Some(rest) = path.strip_prefix('~') {
        if let Ok(home) = std::env::var("HOME") {
            let home = PathBuf::from(home);
            if rest.is_empty() || rest == "/" {
                return home;
            }
            let rest = rest.trim_start_matches('/');
            return home.join(rest);
        }
    }
    PathBuf::from(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_handle_healthz() {
        let body = handle_request("healthz", None);
        assert_eq!(
            body,
            r#"{"ok":true,"service":"agent-sandbox-helper-daemon"}"#
        );
    }

    #[test]
    fn test_handle_healthz_with_slash() {
        let body = handle_request("/healthz", None);
        assert_eq!(
            body,
            r#"{"ok":true,"service":"agent-sandbox-helper-daemon"}"#
        );
    }

    #[test]
    fn test_handle_test_action() {
        let body = handle_request("test", None);
        let v: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(v["ok"].as_bool(), Some(true));
        assert_eq!(
            v["message"].as_str().unwrap(),
            "helper daemon connectivity works"
        );
    }

    #[test]
    fn test_handle_not_found() {
        let body = handle_request("nonexistent", None);
        let v: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert!(!v["ok"].as_bool().unwrap());
        assert_eq!(v["error"].as_str().unwrap(), "not found");
    }

    #[test]
    fn test_handle_empty_line_returns_not_found() {
        let body = handle_request("", None);
        let v: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert!(!v["ok"].as_bool().unwrap());
        assert_eq!(v["error"].as_str().unwrap(), "not found");
    }

    #[test]
    fn test_git_pull_missing_path() {
        let body = handle_request("git-pull", None);
        let v: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert!(!v["ok"].as_bool().unwrap());
        assert_eq!(
            v["error"].as_str().unwrap(),
            "usage: git-pull <absolute-path>"
        );
    }

    #[test]
    fn test_git_pull_relative_path_rejected() {
        let body = handle_request("git-pull relative/path", None);
        let v: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert!(!v["ok"].as_bool().unwrap());
        assert_eq!(v["error"].as_str().unwrap(), "path must be absolute");
    }

    #[test]
    fn test_git_pull_outside_projects_root() {
        let root = PathBuf::from("/allowed");
        let body = handle_request("git-pull /forbidden", Some(root.as_path()));
        let v: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert!(!v["ok"].as_bool().unwrap());
        assert!(v["error"]
            .as_str()
            .unwrap()
            .contains("outside allowed projects root"));
    }

    #[test]
    fn test_git_pull_non_git_dir() {
        let body = handle_request("git-pull /tmp", None);
        let v: serde_json::Value = serde_json::from_str(&body).unwrap();
        // /tmp exists but is not a git repo — git should fail with exit code 128
        assert!(!v["ok"].as_bool().unwrap());
        assert_eq!(v["exit_code"].as_i64(), Some(128));
        assert!(v["stderr"]
            .as_str()
            .unwrap()
            .contains("not a git repository"));
    }

    #[test]
    fn test_handle_healthz_bare_matches_without_http() {
        let body = handle_request("healthz", None);
        assert_eq!(
            body,
            r#"{"ok":true,"service":"agent-sandbox-helper-daemon"}"#
        );
    }

    #[test]
    fn test_resolve_path_tilde() {
        let home = std::env::var("HOME").unwrap();
        assert_eq!(resolve_path("~/foo"), PathBuf::from(home).join("foo"));
    }

    #[test]
    fn test_resolve_path_tilde_only() {
        let home = std::env::var("HOME").unwrap();
        assert_eq!(resolve_path("~"), PathBuf::from(home));
    }

    #[test]
    fn test_resolve_path_absolute() {
        assert_eq!(resolve_path("/tmp/bar"), PathBuf::from("/tmp/bar"));
    }

    #[test]
    fn test_resolve_path_relative() {
        assert_eq!(
            resolve_path("relative/path"),
            PathBuf::from("relative/path")
        );
    }

    // --- git-push guardrail unit tests ---

    #[test]
    fn test_is_protected_branch_main() {
        assert!(is_protected_branch("main"));
    }

    #[test]
    fn test_is_protected_branch_master() {
        assert!(is_protected_branch("master"));
    }

    #[test]
    fn test_is_protected_branch_feature() {
        assert!(!is_protected_branch("feature-x"));
    }

    #[test]
    fn test_is_protected_branch_detached() {
        assert!(!is_protected_branch("HEAD"));
    }

    #[test]
    fn test_git_push_missing_path() {
        let body = handle_request("git-push", None);
        let v: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert!(!v["ok"].as_bool().unwrap());
        assert_eq!(
            v["error"].as_str().unwrap(),
            "usage: git-push <absolute-path>"
        );
    }

    #[test]
    fn test_git_push_relative_path_rejected() {
        let body = handle_request("git-push relative/path", None);
        let v: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert!(!v["ok"].as_bool().unwrap());
        assert_eq!(v["error"].as_str().unwrap(), "path must be absolute");
    }

    #[test]
    fn test_git_push_outside_projects_root() {
        let root = PathBuf::from("/allowed");
        let body = handle_request("git-push /forbidden", Some(root.as_path()));
        let v: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert!(!v["ok"].as_bool().unwrap());
        assert!(v["error"]
            .as_str()
            .unwrap()
            .contains("outside allowed projects root"));
    }

    #[test]
    fn test_git_push_non_git_dir() {
        let body = handle_request("git-push /tmp", None);
        let v: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert!(!v["ok"].as_bool().unwrap());
        let err = v["error"].as_str().unwrap();
        assert!(
            err.contains("not a git repository")
                || err.contains("failed to determine current branch"),
            "expected git error, got: {err}"
        );
    }

    #[test]
    fn test_git_push_rejects_main() {
        // Create a temp git repo on main and verify the guardrail rejects it
        let dir = tempfile::TempDir::new().unwrap();
        let repo = dir.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();

        Command::new("git")
            .args(["init", "-b", "main"])
            .arg(&repo)
            .status()
            .unwrap();

        std::fs::write(repo.join("file"), b"data").unwrap();
        Command::new("git")
            .args(["add", "file"])
            .current_dir(&repo)
            .status()
            .unwrap();
        Command::new("git")
            .args(["commit", "-m", "init"])
            .current_dir(&repo)
            .status()
            .unwrap();

        let body = handle_request(&format!("git-push {}", repo.display()), None);
        let v: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert!(!v["ok"].as_bool().unwrap());
        assert!(v["error"].as_str().unwrap().contains("protected branch"));
    }

    #[test]
    fn test_git_push_rejects_master() {
        let dir = tempfile::TempDir::new().unwrap();
        let repo = dir.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();

        Command::new("git")
            .args(["init", "-b", "master"])
            .arg(&repo)
            .status()
            .unwrap();

        std::fs::write(repo.join("file"), b"data").unwrap();
        Command::new("git")
            .args(["add", "file"])
            .current_dir(&repo)
            .status()
            .unwrap();
        Command::new("git")
            .args(["commit", "-m", "init"])
            .current_dir(&repo)
            .status()
            .unwrap();

        let body = handle_request(&format!("git-push {}", repo.display()), None);
        let v: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert!(!v["ok"].as_bool().unwrap());
        assert!(v["error"].as_str().unwrap().contains("protected branch"));
    }

    // --- pr-create unit tests ---

    #[test]
    fn test_pr_create_bad_json() {
        let body = handle_request("pr-create not-json", None);
        let v: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert!(!v["ok"].as_bool().unwrap());
        assert!(v["error"]
            .as_str()
            .unwrap()
            .contains("invalid pr-create JSON"));
    }

    #[test]
    fn test_pr_create_not_an_object() {
        let body = handle_request(r#"pr-create "string""#, None);
        let v: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert!(!v["ok"].as_bool().unwrap());
        assert!(v["error"]
            .as_str()
            .unwrap()
            .contains("must be a JSON object"));
    }

    #[test]
    fn test_pr_create_missing_field() {
        let body = handle_request(r#"pr-create {"path":"/p","title":"t"}"#, None);
        let v: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert!(!v["ok"].as_bool().unwrap());
        assert!(v["error"]
            .as_str()
            .unwrap()
            .contains("missing required field 'source'"));
    }

    #[test]
    fn test_pr_create_relative_path() {
        let body = handle_request(
            r#"pr-create {"path":"relative/path","title":"t","source":"f"}"#,
            None,
        );
        let v: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert!(!v["ok"].as_bool().unwrap());
        assert_eq!(v["error"].as_str().unwrap(), "path must be absolute");
    }

    #[test]
    fn test_pr_create_outside_projects_root() {
        let root = PathBuf::from("/allowed");
        let body = handle_request(
            r#"pr-create {"path":"/forbidden","title":"t","source":"f"}"#,
            Some(root.as_path()),
        );
        let v: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert!(!v["ok"].as_bool().unwrap());
        assert!(v["error"]
            .as_str()
            .unwrap()
            .contains("outside allowed projects root"));
    }

    #[test]
    fn test_pr_create_non_git_repo() {
        let body = handle_request(
            r#"pr-create {"path":"/tmp","title":"t","source":"f"}"#,
            None,
        );
        let v: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert!(!v["ok"].as_bool().unwrap());
        let err = v["error"].as_str().unwrap();
        assert!(
            err.contains("failed to get remote 'origin'"),
            "expected no-remote error, got: {err}"
        );
    }

    #[test]
    fn test_pr_create_relative_path_no_git() {
        let body = handle_request(
            r#"pr-create {"path":"/nonexistent/path","title":"t","source":"f"}"#,
            None,
        );
        let v: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert!(!v["ok"].as_bool().unwrap());
        let err = v["error"].as_str().unwrap();
        assert!(
            err.contains("failed to execute git") || err.contains("failed to get remote"),
            "expected git error, got: {err}"
        );
    }

    // --- Remote URL parsing tests ---

    #[test]
    fn test_parse_ado_https_simple() {
        let provider =
            try_parse_ado_https("https://dev.azure.com/myorg/myproject/_git/myrepo").unwrap();
        let (org, project, repo) = match provider {
            GitProvider::AzureDevops { org, project, repo } => (org, project, repo),
        };
        assert_eq!(org, "myorg");
        assert_eq!(project, "myproject");
        assert_eq!(repo, "myrepo");
    }

    #[test]
    fn test_parse_ado_https_with_dot_git() {
        let provider =
            try_parse_ado_https("https://dev.azure.com/myorg/myproject/_git/myrepo.git").unwrap();
        let (org, project, repo) = match provider {
            GitProvider::AzureDevops { org, project, repo } => (org, project, repo),
        };
        assert_eq!(org, "myorg");
        assert_eq!(project, "myproject");
        assert_eq!(repo, "myrepo");
    }

    #[test]
    fn test_parse_ado_https_with_userinfo() {
        let provider =
            try_parse_ado_https("https://pat@dev.azure.com/myorg/myproject/_git/myrepo").unwrap();
        let (org, project, repo) = match provider {
            GitProvider::AzureDevops { org, project, repo } => (org, project, repo),
        };
        assert_eq!(org, "myorg");
        assert_eq!(project, "myproject");
        assert_eq!(repo, "myrepo");
    }

    #[test]
    fn test_parse_ado_ssh() {
        let provider =
            try_parse_ado_ssh("git@ssh.dev.azure.com:v3/myorg/myproject/myrepo").unwrap();
        let (org, project, repo) = match provider {
            GitProvider::AzureDevops { org, project, repo } => (org, project, repo),
        };
        assert_eq!(org, "myorg");
        assert_eq!(project, "myproject");
        assert_eq!(repo, "myrepo");
    }

    #[test]
    fn test_parse_ado_https_rejects_non_ado() {
        assert!(try_parse_ado_https("https://github.com/owner/repo").is_none());
    }

    #[test]
    fn test_parse_ado_ssh_rejects_non_ado() {
        assert!(try_parse_ado_ssh("git@github.com:owner/repo.git").is_none());
    }

    #[test]
    fn test_parse_pr_params_all_fields() {
        let params = parse_pr_params(
            r#"{"path":"/repo","title":"My PR","source":"feature","target":"main","description":"desc"}"#,
        )
        .unwrap();
        assert_eq!(params.path, "/repo");
        assert_eq!(params.title, "My PR");
        assert_eq!(params.source, "feature");
        assert_eq!(params.target.as_deref(), Some("main"));
        assert_eq!(params.description.as_deref(), Some("desc"));
    }

    #[test]
    fn test_parse_pr_params_optional_fields_omitted() {
        let params =
            parse_pr_params(r#"{"path":"/repo","title":"PR","source":"feature"}"#).unwrap();
        assert_eq!(params.path, "/repo");
        assert_eq!(params.title, "PR");
        assert_eq!(params.source, "feature");
        assert!(params.target.is_none());
        assert!(params.description.is_none());
    }
}
