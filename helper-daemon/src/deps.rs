use std::path::Path;
use std::process::Command;

use crate::err_response;
use crate::ok_response;

pub fn dep_install(dir: &Path, projects_root: Option<&Path>) -> String {
    if !dir.is_absolute() {
        return err_response("path must be absolute");
    }
    if let Some(root) = projects_root {
        if !dir.starts_with(root) {
            return err_response(format!(
                "path is outside allowed projects root: {}",
                dir.display()
            ));
        }
    }
    let lockfiles = [
        "yarn.lock",
        "package-lock.json",
        "pnpm-lock.yaml",
        "uv.lock",
    ];
    let found_locks: Vec<_> = lockfiles.iter().filter(|f| dir.join(f).exists()).collect();
    let has_csproj = std::fs::read_dir(dir)
        .ok()
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .any(|e| e.path().extension().map_or(false, |ext| ext == "csproj"))
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
    match Command::new(prog).current_dir(dir).args(args).output() {
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
