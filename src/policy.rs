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

/// Recursively walk a JSON value and expand `~` in every string that looks
/// like a filesystem path (strings in denyRead, allowWrite, denyWrite, allowRead).
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
      "~/.ssh",
      "~/.gnupg",
      "~/.aws",
      "~/.azure",
      "~/.config/gh",
      "~/.config/github-copilot",
      "~/.config/gcloud",
      "~/.config/azure",
      "~/.docker",
      "~/.kube",
      "~/.npmrc",
      "~/.pypirc",
      "~/.netrc",
      "~/.cargo/credentials",
      "~/.cargo/credentials.toml",
      "~/.nuget",
      "~/.m2/settings.xml",
      "~/.gradle/gradle.properties",
      "~/.local/share/opencode/auth.json",
      ".env",
      ".env.local",
      ".envrc",
      ".npmrc",
      ".pypirc",
      "NuGet.config",
      "nuget.config"
    ],
    "allowRead": [],
    "allowWrite": [
      ".",
      "~/.agent-sandbox",
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

pub fn render_settings(
    projects_root: &Path,
    daemon_sock: &Path,
    allowed_domains: &[String],
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let home = host_home();
    let mut settings: serde_json::Value = serde_json::from_str(DEFAULT_SETTINGS_JSON)?;

    // Override allowed domains with user config
    if let Some(allowed) = settings
        .pointer_mut("/network/allowedDomains")
        .and_then(|v| v.as_array_mut())
    {
        *allowed = allowed_domains
            .iter()
            .map(|d| serde_json::Value::String(d.clone()))
            .collect();
    }

    // Add the daemon socket's parent dir to allowWrite so bwrap bind-mounts it rw
    if let Some(allow_write) = settings
        .pointer_mut("/filesystem/allowWrite")
        .and_then(|v| v.as_array_mut())
    {
        let sock_dir = daemon_sock.parent().unwrap();
        let abs = sock_dir.to_string_lossy().to_string();
        if !allow_write.iter().any(|v| v.as_str() == Some(&abs)) {
            allow_write.push(abs.into());
        }
    }

    // Expand "." in allowWrite to the resolved projects root
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

    // Expand all ~ paths to absolute paths against the REAL host home,
    // before SRT overrides $HOME to the sandbox home.
    expand_tilde_in_arrays(&mut settings, &home);

    Ok(settings)
}

pub fn prepare_settings(
    projects_root: &Path,
    daemon_sock: &Path,
    allowed_domains: &[String],
) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let settings = render_settings(projects_root, daemon_sock, allowed_domains)?;

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
