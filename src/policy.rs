use anyhow::Result;
use std::env;
use std::path::{Path, PathBuf};

fn host_home() -> PathBuf {
    crate::config::home_dir().expect("HOME must be set before srt spawns")
}

fn expand_tilde(path: &str, home: &Path) -> String {
    if path == "~" {
        return home.to_string_lossy().to_string();
    }
    if let Some(rest) = path.strip_prefix("~/") {
        return home.join(rest).to_string_lossy().to_string();
    }
    path.to_string()
}

fn expand_tilde_in_arrays(value: &mut serde_json::Value, home: &Path) {
    match value {
        serde_json::Value::String(s) => {
            *s = expand_tilde(s, home);
        }
        serde_json::Value::Array(arr) => {
            for item in arr.iter_mut() {
                expand_tilde_in_arrays(item, home);
            }
        }
        serde_json::Value::Object(obj) => {
            for val in obj.values_mut() {
                expand_tilde_in_arrays(val, home);
            }
        }
        _ => {}
    }
}

pub fn discover_allow_read_paths(home: &Path) -> Vec<PathBuf> {
    let mut paths: Vec<PathBuf> = Vec::new();

    if let Some(path_var) = env::var_os("PATH") {
        for entry in env::split_paths(&path_var) {
            if entry.starts_with(home) {
                let resolved = entry.canonicalize().unwrap_or(entry);
                if resolved.exists() {
                    paths.push(resolved);
                }
            }
        }
    }

    for p in &[
        "~/.local/share/mise",
        "~/.local/bin",
        "~/.rustup",
        "~/.nuget",
    ] {
        let exp = expand_tilde(p, home);
        let path = PathBuf::from(exp);
        if path.exists() {
            paths.push(path);
        }
    }

    paths.sort();
    paths.dedup();
    paths
}

pub const DEFAULT_SETTINGS_JSON: &str = r#"{
  "network": {
    "allowedDomains": [
      "api.anthropic.com"
    ],
    "deniedDomains": [],
    "allowUnixSockets": [],
    "allowAllUnixSockets": true,
    "allowLocalBinding": false
  },
  "filesystem": {
    "denyRead": [
      "~"
    ],
    "allowRead": [],
    "allowWrite": [
      ".",
      "~/.agent-sandbox",
      "~/.cargo",
      "~/.pi",
      "~/.config/opencode",
      "~/.local/share/opencode",
      "/tmp",
      "/dev/shm"
    ],
    "denyWrite": [
      ".env",
      ".env.local",
      ".envrc",
      ".npmrc",
      ".pypirc",
      "NuGet.config",
      "nuget.config",
      ".git/config",
      ".git/hooks",
      ".github/workflows"
    ],
    "allowGitConfig": false
  },
  "ignoreViolations": {},
  "mandatoryDenySearchDepth": 5,
  "enableWeakerNestedSandbox": false,
  "enableWeakerNetworkIsolation": false
}
"#;

fn push_to_array(settings: &mut serde_json::Value, path: &str, item: String) {
    if let Some(arr) = settings.pointer_mut(path).and_then(|v| v.as_array_mut()) {
        if !arr.iter().any(|v| v.as_str() == Some(&item)) {
            arr.push(item.into());
        }
    }
}

pub fn render_settings(
    projects_root: &Path,
    daemon_sock: &Path,
    allowed_domains: &[String],
    extra_write_dirs: &[PathBuf],
) -> Result<serde_json::Value> {
    let home = host_home();
    let mut settings: serde_json::Value = serde_json::from_str(DEFAULT_SETTINGS_JSON)?;

    if let Some(allowed) = settings
        .pointer_mut("/network/allowedDomains")
        .and_then(|v| v.as_array_mut())
    {
        *allowed = allowed_domains
            .iter()
            .map(|d| serde_json::Value::String(d.clone()))
            .collect();
    }

    if let Some(sock_dir) = daemon_sock.parent() {
        push_to_array(
            &mut settings,
            "/filesystem/allowWrite",
            sock_dir.to_string_lossy().to_string(),
        );
    }

    if let Some(allow_write) = settings
        .pointer_mut("/filesystem/allowWrite")
        .and_then(|v| v.as_array_mut())
    {
        for item in allow_write.iter_mut() {
            if item.as_str() == Some(".") {
                *item = serde_json::Value::String(projects_root.to_string_lossy().to_string());
            }
        }
    }

    for p in &discover_allow_read_paths(&home) {
        push_to_array(
            &mut settings,
            "/filesystem/allowRead",
            p.to_string_lossy().to_string(),
        );
    }

    let cargo_dir = home.join(".cargo");
    if !cargo_dir.exists() {
        if let Some(allow_write) = settings
            .pointer_mut("/filesystem/allowWrite")
            .and_then(|v| v.as_array_mut())
        {
            allow_write.retain(|v| v.as_str() != Some(cargo_dir.to_string_lossy().as_ref()));
        }
    }

    for extra_dir in extra_write_dirs {
        let s = extra_dir.to_string_lossy().to_string();
        push_to_array(&mut settings, "/filesystem/allowRead", s.clone());
        push_to_array(&mut settings, "/filesystem/allowWrite", s);
    }

    expand_tilde_in_arrays(&mut settings, &home);

    Ok(settings)
}

pub fn prepare_settings(
    projects_root: &Path,
    daemon_sock: &Path,
    allowed_domains: &[String],
    extra_write_dirs: &[PathBuf],
) -> Result<PathBuf> {
    let settings = render_settings(
        projects_root,
        daemon_sock,
        allowed_domains,
        extra_write_dirs,
    )?;

    let tmp_path = std::env::temp_dir().join("agent-sandbox-settings.json");
    std::fs::write(&tmp_path, serde_json::to_string_pretty(&settings)?)?;
    Ok(tmp_path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_expand_tilde_only() {
        let home = Path::new("/home/user");
        assert_eq!(expand_tilde("~", home), "/home/user");
    }

    #[test]
    fn test_expand_tilde_path() {
        let home = Path::new("/home/user");
        assert_eq!(expand_tilde("~/foo/bar", home), "/home/user/foo/bar");
    }

    #[test]
    fn test_expand_tilde_noop() {
        let home = Path::new("/home/user");
        assert_eq!(expand_tilde("/abs/path", home), "/abs/path");
        assert_eq!(expand_tilde("relative/path", home), "relative/path");
    }

    #[test]
    fn test_expand_tilde_in_strings() {
        let home = Path::new("/home/user");
        let mut val = serde_json::json!({
            "filesystem": {
                "denyRead": ["~/.ssh", "~/.aws"],
                "allowWrite": ["."]
            }
        });
        expand_tilde_in_arrays(&mut val, home);
        assert_eq!(
            val.pointer("/filesystem/denyRead/0").unwrap(),
            "/home/user/.ssh"
        );
        assert_eq!(
            val.pointer("/filesystem/denyRead/1").unwrap(),
            "/home/user/.aws"
        );
        assert_eq!(val.pointer("/filesystem/allowWrite/0").unwrap(), ".");
    }
}
