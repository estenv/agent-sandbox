/// Commands sent from the helper CLI to the daemon over the Unix socket.
#[derive(Debug, Clone, PartialEq)]
pub enum DaemonCommand {
    Healthz,
    Test,
    GitPull {
        path: String,
    },
    GitPush {
        path: String,
    },
    PrCreate {
        path: String,
        title: String,
        source: String,
        target: Option<String>,
        description: Option<String>,
    },
    DepInstall {
        path: String,
    },
}

impl DaemonCommand {
    /// Serialize to the line-based wire format.
    pub fn to_wire(&self) -> String {
        match self {
            Self::Healthz => "healthz".into(),
            Self::Test => "test".into(),
            Self::GitPull { path } => format!("git-pull {path}"),
            Self::GitPush { path } => format!("git-push {path}"),
            Self::PrCreate {
                path,
                title,
                source,
                target,
                description,
            } => {
                let mut obj = serde_json::json!({
                    "path": path,
                    "title": title,
                    "source": source,
                });
                if let Some(t) = target {
                    obj["target"] = serde_json::json!(t);
                }
                if let Some(d) = description {
                    obj["description"] = serde_json::json!(d);
                }
                format!("pr-create {obj}")
            }
            Self::DepInstall { path } => format!("dep-install {path}"),
        }
    }

    /// Parse from the line-based wire format.
    pub fn from_wire(s: &str) -> Result<Self, String> {
        let line = s.trim_start_matches('/').trim();
        if line.is_empty() {
            return Err("not found".into());
        }

        let mut parts = line.splitn(2, ' ');
        let action = parts.next().unwrap_or("");
        let arg = parts.next().unwrap_or("").trim();

        match action {
            "healthz" => Ok(Self::Healthz),
            "test" => Ok(Self::Test),
            "git-pull" => {
                if arg.is_empty() {
                    return Err("usage: git-pull <absolute-path>".into());
                }
                Ok(Self::GitPull {
                    path: arg.to_string(),
                })
            }
            "git-push" => {
                if arg.is_empty() {
                    return Err("usage: git-push <absolute-path>".into());
                }
                Ok(Self::GitPush {
                    path: arg.to_string(),
                })
            }
            "pr-create" => {
                let v: serde_json::Value = serde_json::from_str(arg)
                    .map_err(|e| format!("invalid pr-create JSON: {e}"))?;
                let obj = v
                    .as_object()
                    .ok_or_else(|| "pr-create params must be a JSON object".to_string())?;

                let extract = |key: &str| -> Result<String, String> {
                    obj.get(key)
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string())
                        .ok_or_else(|| format!("missing required field '{key}'"))
                };

                Ok(Self::PrCreate {
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
            "dep-install" => {
                if arg.is_empty() {
                    return Err("usage: dep-install <absolute-path>".into());
                }
                Ok(Self::DepInstall {
                    path: arg.to_string(),
                })
            }
            _ => Err("not found".into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_roundtrip_healthz() {
        let cmd = DaemonCommand::Healthz;
        assert_eq!(cmd.to_wire(), "healthz");
        assert_eq!(DaemonCommand::from_wire("healthz").unwrap(), cmd);
    }

    #[test]
    fn test_roundtrip_test() {
        let cmd = DaemonCommand::Test;
        assert_eq!(cmd.to_wire(), "test");
        assert_eq!(DaemonCommand::from_wire("test").unwrap(), cmd);
    }

    #[test]
    fn test_roundtrip_git_pull() {
        let cmd = DaemonCommand::GitPull {
            path: "/repo".into(),
        };
        assert_eq!(cmd.to_wire(), "git-pull /repo");
        assert_eq!(DaemonCommand::from_wire("git-pull /repo").unwrap(), cmd);
    }

    #[test]
    fn test_roundtrip_git_push() {
        let cmd = DaemonCommand::GitPush {
            path: "/repo".into(),
        };
        assert_eq!(cmd.to_wire(), "git-push /repo");
        assert_eq!(DaemonCommand::from_wire("git-push /repo").unwrap(), cmd);
    }

    #[test]
    fn test_roundtrip_pr_create_all_fields() {
        let cmd = DaemonCommand::PrCreate {
            path: "/repo".into(),
            title: "My PR".into(),
            source: "feature".into(),
            target: Some("main".into()),
            description: Some("desc".into()),
        };
        let wire = cmd.to_wire();
        assert!(wire.starts_with("pr-create "));
        assert_eq!(DaemonCommand::from_wire(&wire).unwrap(), cmd);
    }

    #[test]
    fn test_roundtrip_pr_create_optional_omitted() {
        let cmd = DaemonCommand::PrCreate {
            path: "/repo".into(),
            title: "PR".into(),
            source: "feature".into(),
            target: None,
            description: None,
        };
        let wire = cmd.to_wire();
        assert!(wire.starts_with("pr-create "));
        assert_eq!(DaemonCommand::from_wire(&wire).unwrap(), cmd);
    }

    #[test]
    fn test_from_wire_strips_leading_slash() {
        assert_eq!(
            DaemonCommand::from_wire("/healthz").unwrap(),
            DaemonCommand::Healthz
        );
    }

    #[test]
    fn test_from_wire_empty() {
        assert_eq!(DaemonCommand::from_wire(""), Err("not found".into()));
    }

    #[test]
    fn test_from_wire_unknown() {
        assert_eq!(
            DaemonCommand::from_wire("nonexistent"),
            Err("not found".into())
        );
    }

    #[test]
    fn test_from_wire_git_pull_no_path() {
        assert_eq!(
            DaemonCommand::from_wire("git-pull"),
            Err("usage: git-pull <absolute-path>".into())
        );
    }

    #[test]
    fn test_from_wire_git_push_no_path() {
        assert_eq!(
            DaemonCommand::from_wire("git-push"),
            Err("usage: git-push <absolute-path>".into())
        );
    }

    #[test]
    fn test_from_wire_pr_create_bad_json() {
        let err = DaemonCommand::from_wire("pr-create not-json").unwrap_err();
        assert!(err.contains("invalid pr-create JSON"));
    }

    #[test]
    fn test_from_wire_pr_create_not_object() {
        let err = DaemonCommand::from_wire(r#"pr-create "string""#).unwrap_err();
        assert!(err.contains("must be a JSON object"));
    }

    #[test]
    fn test_from_wire_pr_create_missing_field() {
        let err = DaemonCommand::from_wire(r#"pr-create {"path":"/p","title":"t"}"#).unwrap_err();
        assert!(err.contains("missing required field 'source'"));
    }
}
