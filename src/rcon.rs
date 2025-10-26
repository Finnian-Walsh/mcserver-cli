use crate::{
    config,
    error::{Error, Result},
};
use std::{ffi::OsStr, process::Command};

pub fn run<C, T>(server: impl AsRef<str>, commands: C) -> Result<()>
where
    C: AsRef<[T]>,
    T: AsRef<OsStr>,
{
    let config = config::get()?;
    let rcon_config = &config.rcon;

    let server_rcon_config = rcon_config
        .get(server.as_ref())
        .ok_or_else(|| Error::MissingRconConfig(server.as_ref().to_string()))?;

    let mut command = Command::new("mcrcon");

    if let Some(server_address) = &server_rcon_config.server_address {
        command.arg("-H");
        command.arg(server_address);
    }

    if let Some(port) = &server_rcon_config.port {
        command.arg("-P");
        command.arg(port.to_string());
    }

    if let Some(password) = &server_rcon_config.password {
        command.arg("-p");
        command.arg(password);
    }

    for arg in commands.as_ref() {
        command.arg(arg);
    }

    let status = command.status()?;

    if status.success() {
        Ok(())
    } else {
        Err(Error::CommandFailure {
            code: status.code(),
            stderr: None,
        })
    }
}
