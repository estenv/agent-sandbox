use std::path::Path;
use std::time::Duration;

use crate::cmd;
use crate::err_response;
use crate::ok_response;

const INSTALL_TIMEOUT: Duration = Duration::from_secs(300);

pub fn dep_install(cwd: &Path) -> String {
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
    match cmd::run_output(prog, args, cwd, INSTALL_TIMEOUT, prog) {
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
        Err(e) => err_response(e),
    }
}
