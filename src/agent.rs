use anyhow::{anyhow, Result};
use std::env;
use std::ffi::OsString;
use std::fs;
use std::io;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::Command;

struct AgentDef {
    name: &'static str,
    commands: &'static [&'static str],
    package: &'static str,
    env: &'static [(&'static str, &'static str)],
}

const AGENTS: &[AgentDef] = &[
    AgentDef {
        name: "opencode",
        commands: &["opencode"],
        package: "opencode-ai",
        env: &[("OPENCODE_DISABLE_AUTOUPDATE", "true")],
    },
    AgentDef {
        name: "pi",
        commands: &["pi", "pi-agent"],
        package: "@mariozechner/pi-coding-agent",
        env: &[("PI_OFFLINE", "true")],
    },
];

pub fn prepare(agent: &str, sandbox_home: &Path) -> Result<()> {
    let def = AGENTS
        .iter()
        .find(|a| a.name == agent)
        .ok_or_else(|| anyhow!("no preparation recipe for `{agent}`"))?;
    npm_install_prefix(def.package, &sandbox_home.join("npm-prefix"))?;
    symlink_binaries(def.commands, sandbox_home)?;
    Ok(())
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
    let bin = sandbox_home.join("bin").join(command);
    bin.exists()
}

fn npm_install_prefix(package: &str, prefix: &Path) -> io::Result<()> {
    fs::create_dir_all(prefix)?;
    let npm = env::var_os("AGENT_SANDBOX_NPM").unwrap_or_else(|| OsString::from("npm"));
    let status = Command::new(npm)
        .arg("install")
        .arg("--prefix")
        .arg(prefix)
        .arg("-g")
        .arg(package)
        .status()?;
    if status.success() {
        Ok(())
    } else {
        Err(io::Error::other(format!(
            "npm install --prefix {} -g {package} failed",
            prefix.display()
        )))
    }
}

fn symlink_binaries(commands: &[&str], sandbox_home: &Path) -> io::Result<()> {
    let bin_dir = sandbox_home.join("bin");
    fs::create_dir_all(&bin_dir)?;
    let npm_bin = sandbox_home.join("npm-prefix").join("bin");

    for cmd in commands {
        let src = npm_bin.join(cmd);
        let dst = bin_dir.join(cmd);
        if src.exists() {
            let _ = fs::remove_file(&dst);
            fs::copy(&src, &dst)?;
            let mut perms = fs::metadata(&dst)?.permissions();
            perms.set_mode(perms.mode() | 0o111);
            fs::set_permissions(&dst, perms)?;
        }
    }
    Ok(())
}
