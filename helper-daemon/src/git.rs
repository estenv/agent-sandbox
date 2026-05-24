use std::path::Path;
use std::process::Command;

pub fn git_pull(cwd: &Path) -> String {
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
        Err(e) => crate::err_response(format!("failed to execute git: {e}")),
    }
}

pub fn git_push(cwd: &Path) -> String {
    let branch = match current_branch(cwd) {
        Ok(b) => b,
        Err(e) => return crate::err_response(e),
    };

    if is_protected_branch(&branch) {
        return crate::err_response(format!(
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
        Err(e) => crate::err_response(format!("failed to execute git: {e}")),
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

pub fn is_protected_branch(branch: &str) -> bool {
    matches!(branch, "main" | "master")
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn test_git_push_non_git_dir() {
        let dir = tempfile::TempDir::new().unwrap();
        let body = crate::handle_request(&format!("git-push {}", dir.path().display()), None);
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
    fn test_git_pull_non_git_dir() {
        let dir = tempfile::TempDir::new().unwrap();
        let body = crate::handle_request(&format!("git-pull {}", dir.path().display()), None);
        let v: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert!(!v["ok"].as_bool().unwrap());
        assert_eq!(v["exit_code"].as_i64(), Some(128));
        assert!(v["stderr"]
            .as_str()
            .unwrap()
            .contains("not a git repository"));
    }
}
