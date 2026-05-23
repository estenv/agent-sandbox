use std::path::{Path, PathBuf};

fn host_home() -> PathBuf {
    // The real host home — we must NOT use $HOME (which is overridden to the
    // sandbox home by the time srt reads the settings), so read /etc/passwd or
    // fall back to the env var that was set before launch.
    if let Ok(home) = std::env::var("HOME") {
        PathBuf::from(home)
    } else {
        PathBuf::from("/home/as")
    }
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

pub fn prepare_settings(
    base_path: &Path,
    projects_root: &Path,
    daemon_sock: &Path,
) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let home = host_home();
    let content = std::fs::read_to_string(base_path)?;
    let mut settings: serde_json::Value = serde_json::from_str(&content)?;

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

    let tmp_dir = std::env::temp_dir().join(format!("agent-sandbox-{}", std::process::id()));
    std::fs::create_dir_all(&tmp_dir)?;
    let tmp_path = tmp_dir.join("settings.json");
    std::fs::write(&tmp_path, serde_json::to_string_pretty(&settings)?)?;
    Ok(tmp_path)
}

