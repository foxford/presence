use anyhow::{anyhow, Context, Result};
use std::{
    net::IpAddr,
    process::{Command, Stdio},
};
use tracing::error;

pub fn get_local_ip() -> Result<IpAddr> {
    let output = if cfg!(target_os = "linux") {
        Command::new("hostname").arg("-I").output().map_err(|err| {
            error!(error = %err, "Failed to run hostname");
            err
        })?
    } else {
        let mut ifconfig = Command::new("ifconfig")
            .arg("-l")
            .stdout(Stdio::piped())
            .spawn()?;

        let output = ifconfig
            .stdout
            .take()
            .ok_or_else(|| anyhow!("Failed to run ifconfig"))?;

        Command::new("xargs")
            .args(["-n1", "ipconfig", "getifaddr"])
            .stdin(output)
            .output()?
    };

    let ip = String::from_utf8(output.stdout)
        .map_err(|err| {
            error!(error = %err, "Failed to convert stdout to string");
            err
        })
        .context("Failed to convert stdout to string")?;

    let ip = ip
        .replace('\n', "")
        .trim_end()
        .parse::<IpAddr>()
        .context("Failed to parse ip from string")?;

    Ok(ip)
}
