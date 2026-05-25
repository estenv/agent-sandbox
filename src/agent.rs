use anyhow::{anyhow, Context, Result};
use std::env;
use std::ffi::OsString;
use std::fs;
use std::os::unix;
use std::path::Path;
use std::process::Command;

struct AgentDef {
    name: &'static str,
    commands: &'static [&'static str],
    package: &'static str,
    env: &'static [(&'static str, &'static str)],
    /// Pairs of (sandbox_rel, host_rel) to symlink from host ~ into sandbox workspace.
    shared_dirs: &'static [(&'static str, &'static str)],
}

const AGENTS: &[AgentDef] = &[
    AgentDef {
        name: "pi",
        commands: &["pi", "pi-agent"],
        package: "@earendil-works/pi-coding-agent",
        env: &[("PI_OFFLINE", "true")],
        shared_dirs: &[("home/.pi", ".pi")],
    },
    AgentDef {
        name: "opencode",
        commands: &["opencode"],
        package: "opencode-ai",
        env: &[("OPENCODE_DISABLE_AUTOUPDATE", "true")],
        shared_dirs: &[
            ("config/opencode", ".config/opencode"),
            ("share/opencode", ".local/share/opencode"),
        ],
    },
];

struct ToolDef {
    name: &'static str,
    package: &'static str,
}

const TOOLS: &[ToolDef] = &[ToolDef {
    name: "codegraph",
    package: "@colbymchenry/codegraph",
}];

pub fn prepare(agent: &str, sandbox_home: &Path, host_home: &Path) -> Result<()> {
    let def = AGENTS
        .iter()
        .find(|a| a.name == agent)
        .ok_or_else(|| anyhow!("no preparation recipe for `{agent}`"))?;
    npm_install_prefix(def.package, &sandbox_home.join("npm-prefix"))?;
    for (sandbox_rel, host_rel) in def.shared_dirs {
        let target = host_home.join(host_rel);
        let link = sandbox_home.join(sandbox_rel);
        if let Some(parent) = link.parent() {
            fs::create_dir_all(parent)?;
        }
        remove_any(&link)?;
        unix::fs::symlink(&target, &link)?;
    }
    Ok(())
}

fn remove_any(path: &Path) -> Result<()> {
    if path.is_symlink() || !path.is_dir() {
        let _ = fs::remove_file(path);
    } else {
        fs::remove_dir_all(path)?;
    }
    Ok(())
}

pub fn prepare_tool(tool: &str, sandbox_home: &Path) -> Result<()> {
    let def = TOOLS
        .iter()
        .find(|t| t.name == tool)
        .ok_or_else(|| anyhow!("no preparation recipe for tool `{tool}`"))?;
    npm_install_prefix(def.package, &sandbox_home.join("npm-prefix"))?;
    Ok(())
}

/// All unique host-relative paths (~/...) that agents need shared from the host.
pub fn all_shared_host_dirs() -> Vec<&'static str> {
    let mut paths = Vec::new();
    for agent in AGENTS {
        for (_, host_rel) in agent.shared_dirs {
            if !paths.contains(host_rel) {
                paths.push(host_rel);
            }
        }
    }
    paths
}

pub fn known_for_command(command: &str) -> Option<&'static str> {
    AGENTS
        .iter()
        .find(|a| a.commands.contains(&command))
        .map(|a| a.name)
}

pub fn env_vars(agent: &str) -> &[(&'static str, &'static str)] {
    AGENTS
        .iter()
        .find(|a| a.name == agent)
        .map(|a| a.env)
        .unwrap_or(&[])
}

pub fn is_prepared(command: &str, sandbox_home: &Path) -> bool {
    sandbox_home
        .join("npm-prefix")
        .join("bin")
        .join(command)
        .exists()
}

fn npm_install_prefix(package: &str, prefix: &Path) -> Result<()> {
    fs::create_dir_all(prefix)?;
    let npm = env::var_os("AGENT_SANDBOX_NPM").unwrap_or(OsString::from("npm"));
    let status = Command::new(npm)
        .arg("install")
        .arg("--prefix")
        .arg(prefix)
        .arg("-g")
        .arg(package)
        .status()
        .context("failed to execute npm install")?;
    if status.success() {
        Ok(())
    } else {
        Err(anyhow!(
            "npm install --prefix {} -g {package} failed",
            prefix.display()
        ))
    }
}
