use anyhow::{anyhow, Result};
use std::fs;
use std::os::unix;
use std::path::Path;

#[allow(dead_code)]
struct AgentDef {
    name: &'static str,
    commands: &'static [&'static str],
    env: &'static [(&'static str, &'static str)],
    shared_data: &'static [(&'static str, &'static str)],
}

const AGENTS: &[AgentDef] = &[
    AgentDef {
        name: "pi",
        commands: &["pi", "pi-agent"],
        env: &[("PI_OFFLINE", "true")],
        shared_data: &[],
    },
    AgentDef {
        name: "opencode",
        commands: &["opencode"],
        env: &[("OPENCODE_DISABLE_AUTOUPDATE", "true")],
        shared_data: &[("share/opencode", ".local/share/opencode")],
    },
];

pub fn prepare(agent: &str, sandbox_home: &Path, host_home: &Path) -> Result<()> {
    let def = AGENTS
        .iter()
        .find(|a| a.name == agent)
        .ok_or_else(|| anyhow!("no preparation recipe for `{agent}`"))?;
    for (sandbox_rel, host_rel) in def.shared_data {
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

#[allow(dead_code)]
pub fn env_vars(agent: &str) -> &'static [(&'static str, &'static str)] {
    AGENTS
        .iter()
        .find(|a| a.name == agent)
        .map(|a| a.env)
        .unwrap_or(&[])
}
