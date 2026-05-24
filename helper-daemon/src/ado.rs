use std::path::Path;
use std::process::Command;

#[derive(Debug)]
pub struct PrParams {
    pub path: String,
    pub title: String,
    pub source: String,
    pub target: Option<String>,
    pub description: Option<String>,
    pub work_item: Option<i64>,
}

enum GitProvider {
    AzureDevops {
        org: String,
        project: String,
        repo: String,
    },
}

pub fn pr_create(params: &PrParams, projects_root: Option<&Path>) -> String {
    let cwd = Path::new(&params.path);
    if !cwd.is_absolute() {
        return crate::err_response("path must be absolute");
    }
    if let Some(root) = projects_root {
        if !cwd.starts_with(root) {
            return crate::err_response(format!(
                "path is outside allowed projects root: {}",
                cwd.display()
            ));
        }
    }

    let provider = match detect_provider(cwd) {
        Ok(p) => p,
        Err(e) => return crate::err_response(e),
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
                        if let Some(wi) = params.work_item {
                            let _ = link_pr_work_item(org, project, pr_id, wi);
                        }
                        crate::ok_response(serde_json::json!({
                            "provider": "azure-devops",
                            "pull_request_id": pr_id,
                            "url": url,
                        }))
                    }
                    Err(_) => crate::ok_response(serde_json::json!({
                        "provider": "azure-devops",
                        "raw_output": stdout,
                    })),
                }
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr);
                crate::err_response(format!(
                    "az repos pull-request create failed (exit {exit_code}): {}",
                    stderr.trim()
                ))
            }
        }
        Err(e) => crate::err_response(format!("failed to execute az: {e}")),
    }
}

pub fn wi_list(path: &str, projects_root: Option<&Path>) -> String {
    let cwd = Path::new(path);
    if !cwd.is_absolute() {
        return crate::err_response("path must be absolute");
    }
    if let Some(root) = projects_root {
        if !cwd.starts_with(root) {
            return crate::err_response(format!(
                "path is outside allowed projects root: {}",
                cwd.display()
            ));
        }
    }

    let provider = match detect_provider(cwd) {
        Ok(p) => p,
        Err(e) => return crate::err_response(e),
    };

    match provider {
        GitProvider::AzureDevops { org, .. } => query_ado_workitems(&org),
    }
}

fn query_ado_workitems(org: &str) -> String {
    let wiql = "SELECT [System.Id], [System.Title], [System.State], [System.AssignedTo] FROM WorkItems WHERE [System.AssignedTo] = @Me ORDER BY [System.ChangedDate] DESC";
    match Command::new("az")
        .args([
            "boards",
            "query",
            "--org",
            &format!("https://dev.azure.com/{org}"),
            "--wiql",
            wiql,
            "--output",
            "json",
        ])
        .output()
    {
        Ok(output) => {
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                crate::ok_response(
                    serde_json::json!({ "provider": "azure-devops", "workitems": stdout.trim() }),
                )
            } else {
                let stderr = String::from_utf8_lossy(&output.stderr);
                crate::err_response(format!("az boards query failed: {}", stderr.trim()))
            }
        }
        Err(e) => crate::err_response(format!("failed to execute az: {e}")),
    }
}

pub fn wi_create(
    path: &str,
    title: &str,
    parent: Option<i64>,
    description: Option<&str>,
    r#type: Option<&str>,
    projects_root: Option<&Path>,
) -> String {
    let cwd = Path::new(path);
    if !cwd.is_absolute() {
        return crate::err_response("path must be absolute");
    }
    if let Some(root) = projects_root {
        if !cwd.starts_with(root) {
            return crate::err_response(format!(
                "path is outside allowed projects root: {}",
                cwd.display()
            ));
        }
    }

    let provider = match detect_provider(cwd) {
        Ok(p) => p,
        Err(e) => return crate::err_response(e),
    };

    match provider {
        GitProvider::AzureDevops {
            org,
            project: _,
            repo: _,
        } => create_ado_workitem(&org, title, parent, description, r#type),
    }
}

fn create_ado_workitem(
    org: &str,
    title: &str,
    parent: Option<i64>,
    description: Option<&str>,
    r#type: Option<&str>,
) -> String {
    let typ = r#type.unwrap_or("Task");
    let mut cmd = Command::new("az");
    cmd.arg("boards")
        .arg("work-item")
        .arg("create")
        .arg("--org")
        .arg(format!("https://dev.azure.com/{org}"))
        .arg("--type")
        .arg(typ)
        .arg("--title")
        .arg(title)
        .arg("--output")
        .arg("json");

    if let Some(p) = parent {
        cmd.arg("--parents").arg(p.to_string());
    }
    if let Some(d) = description {
        cmd.arg("--description").arg(d);
    }

    match cmd.output() {
        Ok(out) => {
            if out.status.success() {
                let stdout = String::from_utf8_lossy(&out.stdout);
                crate::ok_response(
                    serde_json::json!({"provider":"azure-devops","raw":stdout.trim()}),
                )
            } else {
                let stderr = String::from_utf8_lossy(&out.stderr);
                crate::err_response(format!("az work-item create failed: {}", stderr.trim()))
            }
        }
        Err(e) => crate::err_response(format!("failed execute az: {e}")),
    }
}

fn link_pr_work_item(org: &str, _project: &str, pr_id: i64, wi_id: i64) -> Result<(), String> {
    let status = Command::new("az")
        .args([
            "repos",
            "pr",
            "work-item",
            "add",
            "--org",
            &format!("https://dev.azure.com/{org}"),
            "--id",
            &pr_id.to_string(),
            "--work-items",
            &wi_id.to_string(),
        ])
        .status()
        .map_err(|e| format!("az link failed exec: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err("az pr work-item add non-zero".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_pr_create_bad_json() {
        let body = crate::handle_request("pr-create not-json", None);
        let v: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert!(!v["ok"].as_bool().unwrap());
        assert!(v["error"]
            .as_str()
            .unwrap()
            .contains("invalid pr-create JSON"));
    }

    #[test]
    fn test_pr_create_not_an_object() {
        let body = crate::handle_request(r#"pr-create "string""#, None);
        let v: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert!(!v["ok"].as_bool().unwrap());
        assert!(v["error"]
            .as_str()
            .unwrap()
            .contains("must be a JSON object"));
    }

    #[test]
    fn test_pr_create_missing_field() {
        let body = crate::handle_request(r#"pr-create {"path":"/p","title":"t"}"#, None);
        let v: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert!(!v["ok"].as_bool().unwrap());
        assert!(v["error"]
            .as_str()
            .unwrap()
            .contains("missing required field 'source'"));
    }

    #[test]
    fn test_pr_create_relative_path() {
        let body = crate::handle_request(
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
        let body = crate::handle_request(
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
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().display().to_string();
        let body = crate::handle_request(
            &format!(r#"pr-create {{"path":"{path}","title":"t","source":"f"}}"#),
            None,
        );
        let v: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert!(!v["ok"].as_bool().unwrap());
        let err = v["error"].as_str().unwrap();
        assert!(
            err.contains("failed to get remote 'origin'") || err.contains("failed to execute git"),
            "expected no-remote or git error, got: {err}"
        );
    }

    #[test]
    fn test_pr_create_path_does_not_exist() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("nonexistent-subdir");
        let body = crate::handle_request(
            &format!(
                r#"pr-create {{"path":"{}","title":"t","source":"f"}}"#,
                path.display()
            ),
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
}
