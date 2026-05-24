use std::path::Path;
use std::process::Command;

use crate::err_response;
use crate::ok_response;

pub fn dep_install(dir: &Path, projects_root: Option<&Path>) -> String {
    let cwd = match crate::validate_path(dir.to_str().unwrap_or_default(), projects_root) {
        Ok(d) => d,
        Err(e) => return err_response(e),
    };
    let lockfiles = [
        "yarn.lock",
        "package-lock.json",
        "pnpm-lock.yaml",
        "uv.lock",
    ];
    let found_locks: Vec<_> = lockfiles.iter().filter(|f| cwd.join(f).exists()).collect();
    let has_csproj = std::fs::read_dir(cwd)
        .ok()
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .any(|e| e.path().extension().is_some_and(|ext| ext == "csproj"))
        })
        .unwrap_or(false);
    if found_locks.len() > 1 {
        return err_response("multiple lockfiles, ambiguous package manager");
    }
    let (prog, args): (&str, &[&str]) = if let Some(&lock) = found_locks.first() {
        match *lock {
            "yarn.lock" => ("yarn", &["install"]),
            "package-lock.json" => ("npm", &["install"]),
            "pnpm-lock.yaml" => ("pnpm", &["install"]),
            "uv.lock" => ("uv", &["sync"]),
            _ => unreachable!(),
        }
    } else if has_csproj {
        ("dotnet", &["restore"])
    } else {
        return err_response("no recognizable lockfile or .csproj found");
    };
    match Command::new(prog).current_dir(cwd).args(args).output() {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            let stderr = String::from_utf8_lossy(&out.stderr);
            if out.status.success() {
                ok_response(
                    serde_json::json!({"ok": true, "installed": prog, "stdout": stdout, "stderr": stderr}),
                )
            } else {
                err_response(format!("{} failed: stdout={stdout} stderr={stderr}", prog))
            }
        }
        Err(e) => err_response(format!("failed to run {}: {}", prog, e)),
    }
}
