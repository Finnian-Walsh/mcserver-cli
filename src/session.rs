use crate::{
    error::{Error, Result},
    server::{LastUsed, ServerObject, get_last_used, save_last_used_now},
    session,
};
use std::{
    collections::{HashMap, HashSet},
    ffi::OsStr,
    fmt::Display,
    io::{self, Read, Write},
    path::Path,
    process::{Command, Stdio},
    thread,
    time::Duration,
};

pub const BASE_COMMAND: &str = "zellij";
pub const SUFFIX: &str = ".mcserver";

pub fn get_name(server: impl Display) -> String {
    format!("{server}{SUFFIX}")
}

fn get_server_sessions_raw_string() -> Result<Option<String>> {
    let output = Command::new(BASE_COMMAND).arg("list-sessions").output()?;

    match output.status.code() {
        Some(0) => Ok(Some(String::from_utf8_lossy(&output.stdout).to_string())),
        Some(1) => Ok(None), // no sessions
        _ => Err(Error::CommandFailure {
            code: output.status.code(),
            stderr: Some(output.stderr),
        }),
    }
}

fn session_has_exited(session_line: &&str) -> bool {
    let bracket_pos = match session_line.rfind('(') {
        Some(pos) => pos,
        None => return false,
    };

    session_line[bracket_pos..].contains("EXITED") // if there is no "EXITED", still alive
}

fn session_is_alive(session_line: &&str) -> bool {
    !session_has_exited(session_line)
}

fn session_info_to_server(session_info: &str) -> Option<String> {
    let session_name = match session_info.rfind("[Created") {
        Some(pos) => &session_info[7..=pos - 5],
        None => return None, // unexpected error
    };

    session_name.strip_suffix(session::SUFFIX).map(String::from)
}

fn get_alive_server_sessions() -> Result<HashSet<String>> {
    Ok(get_server_sessions_raw_string()?
        .map(|server_sessions| {
            server_sessions
                .lines()
                .filter(session_is_alive)
                .filter_map(session_info_to_server)
                .collect()
        })
        .unwrap_or_default())
}

fn get_dead_server_sessions() -> Result<HashSet<String>> {
    Ok(get_server_sessions_raw_string()?
        .map(|server_sessions| {
            server_sessions
                .lines()
                .filter(session_has_exited)
                .filter_map(session_info_to_server)
                .collect()
        })
        .unwrap_or_default())
}

fn get_server_sessions_to_living() -> Result<HashMap<String, bool>> {
    Ok(get_server_sessions_raw_string()?
        .map(|ss| {
            ss.lines()
                .map(|s| (s, session_is_alive(&s)))
                .filter_map(|(session, living)| {
                    session_info_to_server(session).map(|server| (server, living))
                })
                .collect()
        })
        .unwrap_or_default())
}

fn add_last_used_tag(server: &mut ServerObject) {
    let last_used = get_last_used(&server.name);

    server
        .tags
        .push(match last_used.unwrap_or(LastUsed::Unknown) {
            LastUsed::Never => format!("(Last used \x1b[35;1mnever\x1b[0m)"),
            LastUsed::Unknown => "(Last used unknown)".to_string(),
            LastUsed::Time(time) => format!("(Last used \x1b[35;1m{time}\x1b[0m ago)"),
        });
}

fn tag_as_active(server: &mut ServerObject) {
    server.tags.push("(\x1b[32;1mactive\x1b[0m)".to_string());
}

fn tag_as_dead(server: &mut ServerObject) {
    server.tags.push("(\x1b[31;1mdead\x1b[0m)".to_string())
}

pub fn retain_active_servers(servers: &mut Vec<ServerObject>) -> Result<()> {
    let sessions = get_alive_server_sessions()?;
    servers.retain(|server| sessions.contains(&server.name));
    Ok(())
}

pub fn retain_inactive_servers(servers: &mut Vec<ServerObject>) -> Result<()> {
    let sessions = get_alive_server_sessions()?;
    servers.retain(|server| !sessions.contains(&server.name));
    servers.iter_mut().for_each(add_last_used_tag);
    Ok(())
}

pub fn retain_dead_servers(servers: &mut Vec<ServerObject>) -> Result<()> {
    let dead_sessions = get_dead_server_sessions()?;
    servers.retain(|server| dead_sessions.contains(&server.name));
    servers.iter_mut().for_each(add_last_used_tag);
    Ok(())
}

pub fn tag_servers(servers: &mut [ServerObject]) -> Result<()> {
    let mapped_sessions = get_server_sessions_to_living()?;

    servers
        .iter_mut()
        .for_each(|server| match mapped_sessions.get(&server.name) {
            Some(true) => tag_as_active(server),
            Some(false) => {
                add_last_used_tag(server);
                tag_as_dead(server);
            }
            None => add_last_used_tag(server),
        });
    Ok(())
}

pub fn tag_dead_servers(servers: &mut [ServerObject]) -> Result<()> {
    let sessions = get_alive_server_sessions()?;

    servers.iter_mut().for_each(|server| {
        if sessions.contains(&server.name) {
            tag_as_dead(server);
        }
    });

    Ok(())
}

pub fn attach(server: impl AsRef<str>) -> Result<()> {
    let server = server.as_ref();
    let mut child = Command::new(BASE_COMMAND)
        .arg("attach")
        .arg(get_name(server))
        .stderr(Stdio::piped())
        .spawn()?;

    let status = child.wait()?;

    if status.success() {
        save_last_used_now(server)
    } else {
        let mut buf = Vec::new();
        child
            .stderr
            .take()
            .ok_or(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "Failed to take stderr pipe",
            ))?
            .read_to_end(&mut buf)?;

        Err(Error::CommandFailure {
            code: status.code(),
            stderr: Some(buf),
        })
    }
}

pub fn new_session<S, I>(session: S, initial_command: Option<I>) -> Result<()>
where
    S: AsRef<OsStr>,
    I: AsRef<OsStr>,
{
    Command::new(BASE_COMMAND)
        .arg("delete-session")
        .arg(&session)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()?;

    let mut command = Command::new(BASE_COMMAND);
    command.arg("--session").arg(&session);
    let mut child = command.spawn()?;

    thread::sleep(Duration::from_millis(300));

    if let Some(command) = initial_command {
        write_line(&session, command)?;
    }

    child.wait()?;

    Ok(())
}

pub fn new_server(
    server: impl Display + AsRef<Path>,
    initial_command: Option<impl AsRef<OsStr>>,
) -> Result<()> {
    save_last_used_now(&server)?;
    let session_name = get_name(&server);
    new_session(session_name, initial_command)?;
    save_last_used_now(&server)
}

pub fn delete_server_session(server: impl Display, force: bool) -> Result<()> {
    let mut command = Command::new(BASE_COMMAND);
    command.arg("delete-session");
    command.arg(format!("{server}{SUFFIX}"));

    if force {
        command.arg("--force");
    }

    command.status()?;
    Ok(())
}

pub fn delete_all() -> Result<()> {
    for session in get_dead_server_sessions()? {
        delete_server_session(session, false)?;
    }

    Ok(())
}

pub fn delete_all_confirmed() -> Result<()> {
    loop {
        print!("Delete all sessions? (y/n): ");
        io::stdout().flush()?;

        let mut confirmation = String::new();
        io::stdin().read_line(&mut confirmation)?;

        match confirmation.trim_end().to_lowercase().as_str() {
            "y" | "yes" => break delete_all()?,
            "n" | "no" => {
                println!("Operation canceled");
                break;
            }
            _ => {}
        };
    }

    Ok(())
}

fn session_write(
    session: impl AsRef<OsStr>,
    mode: &'static str,
    chars: impl AsRef<OsStr>,
) -> Result<()> {
    let status = Command::new(BASE_COMMAND)
        .arg("--session")
        .arg(session)
        .arg("action")
        .arg(mode)
        .arg(chars)
        .spawn()?
        .wait()?;

    if !status.success() {
        return Err(Error::CommandFailure {
            code: status.code(),
            stderr: None,
        });
    }

    Ok(())
}

pub fn write_chars(session: impl AsRef<OsStr>, chars: impl AsRef<OsStr>) -> Result<()> {
    session_write(session, "write-chars", chars)
}

pub fn write_line(session: impl AsRef<OsStr>, chars: impl AsRef<OsStr>) -> Result<()> {
    write_chars(&session, chars)?;
    session_write(&session, "write", "13")?; // 13 is for carriage return
    Ok(())
}
