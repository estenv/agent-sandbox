use std::env;
use std::ffi::OsString;
use std::io;
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

pub fn prepare(agent: &str) -> Result<(), Box<dyn std::error::Error>> {
    let def = AGENTS
        .iter()
        .find(|a| a.name == agent)
        .ok_or_else(|| format!("no preparation recipe for `{agent}`"))?;
    npm_install_global(def.package)?;
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

fn npm_install_global(package: &str) -> io::Result<()> {
    let npm = env::var_os("AGENT_SANDBOX_NPM").unwrap_or_else(|| OsString::from("npm"));
    let status = Command::new(npm)
        .arg("install")
        .arg("-g")
        .arg(package)
        .status()?;
    if status.success() {
        Ok(())
    } else {
        Err(io::Error::other(format!("npm install -g {package} failed")))
    }
}
