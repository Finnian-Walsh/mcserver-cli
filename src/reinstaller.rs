use std::{ffi::OsStr, io, process::Command};

pub static REPO_URL: &str = env!("CARGO_PKG_REPOSITORY");

pub fn with_git(commit: Option<String>) -> io::Result<()> {
    Command::new("cargo")
        .arg("install")
        .arg("--git")
        .arg(if let Some(commit) = commit {
            format!("{REPO_URL}/commit/{commit}")
        } else {
            REPO_URL.to_string()
        })
        .arg("--force")
        .spawn()?
        .wait()?;

    Ok(())
}

pub fn with_path(path: impl AsRef<OsStr>) -> io::Result<()> {
    Command::new("cargo")
        .arg("install")
        .arg("--path")
        .arg(path)
        .arg("--force")
        .spawn()?
        .wait()?;

    Ok(())
}

pub fn with_crate() -> io::Result<()> {
    Command::new("cargo")
        .arg("install")
        .arg(env!("CARGO_PKG_NAME"))
        .spawn()?
        .wait()?;

    Ok(())
}
