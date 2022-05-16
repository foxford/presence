use anyhow::{anyhow, Context, Result};
use std::net::IpAddr;
use std::process::{Command, Stdio};

pub fn get_local_ip() -> Result<IpAddr> {
    let output = if cfg!(target_os = "linux") {
        Command::new("hostname").arg("-I").output()?
    } else {
        let mut ifconfig = Command::new("ifconfig")
            .args(["-l"])
            .stdout(Stdio::piped())
            .spawn()?;

        let output = ifconfig
            .stdout
            .take()
            .ok_or(anyhow!("Failed to run ifconfig"))?;

        Command::new("xargs")
            .args(["-n1", "ipconfig", "getifaddr"])
            .stdin(output)
            .output()?
    };

    let ip = String::from_utf8(output.stdout).context("Failed to convert stdout to string")?;
    let ip = ip
        .replace("\n", "")
        .parse::<IpAddr>()
        .context("Failed to parse ip from string")?;

    Ok(ip)
}
