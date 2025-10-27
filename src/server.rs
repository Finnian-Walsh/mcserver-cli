use crate::{
    config::{self, get_expanded_servers_dir, server_or_current},
    error::{Error, Result},
    platforms::{self, Platform},
    session,
};
use reqwest::{
    blocking::{self, Response},
    header,
};
use std::{
    collections::HashSet,
    env,
    ffi::OsStr,
    fmt::{self, Display, Formatter},
    fs::{self, File},
    io::{self, Write},
    path::{Path, PathBuf},
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};
use url::Url;

const REPO_URL: &str = env!("CARGO_PKG_REPOSITORY");
const TEMPLATE_SUFFIX: &str = ".template";

const METADATA_DIRECTORY: &str = ".mcserver";
const JAR_FILE_TXT_NAME: &str = "jar_file.txt";
const LAST_USED_FILE: &str = "last_used.timestamp";

pub struct ServerObject {
    pub name: String,
    pub tags: Vec<String>,
}

impl ServerObject {
    pub fn new(name: String) -> Self {
        ServerObject { name, tags: vec![] }
    }
}

impl Display for ServerObject {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.name)?;
        for tag in &self.tags {
            write!(f, " {tag}")?;
        }
        Ok(())
    }
}

pub fn copy_directory(src: impl AsRef<Path>, dst: impl AsRef<Path>) -> io::Result<()> {
    fs::create_dir_all(&dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let file_type = entry.file_type()?;

        if file_type.is_dir() {
            copy_directory(entry.path(), dst.as_ref().join(entry.file_name()))?;
        } else {
            fs::copy(entry.path(), dst.as_ref().join(entry.file_name()))?;
        }
    }

    Ok(())
}

pub fn remove_dir_with_retries(dir: impl AsRef<Path>) -> Result<()> {
    const ATTEMPTS: u8 = 10;

    for i in 1..=ATTEMPTS {
        if let Err(err) = fs::remove_dir_all(&dir) {
            if i == ATTEMPTS {
                return Err(Error::Io(err));
            }
        } else {
            return Ok(());
        }
    }

    unreachable!("Code returns before the for loop ends")
}

fn remove_server(server: String) -> Result<()> {
    remove_dir_with_retries(get_expanded_servers_dir()?.join(server))?;
    Ok(())
}

pub fn remove_servers(servers: Vec<String>) -> Result<()> {
    let all_servers = get_all_hashed()?;

    for server in servers {
        let server = server_or_current(server)?;

        if all_servers.get(&server).as_ref().is_none() {
            return Err(Error::ServerNotFound(server));
        }

        remove_server(server)?;
    }

    Ok(())
}

pub fn remove_servers_with_confirmation(servers: Vec<String>) -> Result<()> {
    let all_servers = get_all_hashed()?;

    for server in servers {
        if all_servers.get(&server).as_ref().is_none() {
            return Err(Error::ServerNotFound(server));
        }

        if loop {
            print!("Enter `{server}` to delete the server or nothing to cancel operation: ");
            io::stdout().flush()?;

            let mut response = String::new();
            io::stdin().read_line(&mut response)?;

            if server == response.trim_end() {
                break true;
            } else if response.is_empty() {
                break false;
            }
        } {
            remove_server(server)?;
            println!("Server successfully removed");
        } else {
            println!("Operation canceled");
        }
    }

    Ok(())
}

fn set_last_used_metadata(metadata_dir: impl AsRef<Path>, timestamp: u64) -> Result<()> {
    let mut file = File::create(metadata_dir.as_ref().join(LAST_USED_FILE))?;
    file.write_all(&timestamp.to_le_bytes())?;

    Ok(())
}

pub fn set_jar_file_metadata<M, J>(metadata_dir: M, jar_file_name: J) -> Result<File>
where
    M: AsRef<Path>,
    J: Display,
{
    let mut jar_file_txt = File::create(metadata_dir.as_ref().join(JAR_FILE_TXT_NAME))?;
    writeln!(jar_file_txt, "{jar_file_name}")?;
    Ok(jar_file_txt)
}

pub fn set_default_metadata<M, J>(metadata_dir: M, jar_file_name: J) -> Result<()>
where
    M: AsRef<Path>,
    J: Display,
{
    fs::create_dir_all(&metadata_dir)?;

    let jar_file_txt = set_jar_file_metadata(&metadata_dir, jar_file_name)?;

    let mut perms = jar_file_txt.metadata()?.permissions();
    perms.set_readonly(true);
    jar_file_txt.set_permissions(perms)?;

    set_last_used_metadata(&metadata_dir, u64::MAX)?;

    Ok(())
}

fn copy_jar<S, J, F>(server_dir: S, mut jar: J, file_name: F) -> Result<()>
where
    S: AsRef<Path>,
    J: io::Read,
    F: AsRef<Path>,
{
    env::set_current_dir(server_dir)?;

    let mut jar_file = File::create(file_name)?;
    io::copy(&mut jar, &mut jar_file)?;

    Ok(())
}

pub fn get_jar(download_url: Url, platform: Platform) -> Result<(Response, String)> {
    println!("Downloading from {download_url}...");
    let response = blocking::get(download_url)?;

    let file_name = response
        .headers()
        .get(header::CONTENT_DISPOSITION)
        .map(|disposition| disposition.to_str())
        .transpose()?
        .and_then(|cd| cd.split("filename=\"").nth(1))
        .and_then(|slice| slice.split('"').next())
        .map(String::from)
        .unwrap_or_else(|| format!("{platform}.jar"));

    // if let Err(err) = copy_jar(&server_dir, file_name, response) {
    //     remove_dir_with_retries(server_dir)?;
    //     return Err(err);
    // }

    Ok((response, file_name))
}

pub fn create_new<N>(platform: Platform, version: Option<String>, name: Option<N>) -> Result<()>
where
    N: Display,
{
    let download_url = platforms::get(platform, version)?;

    let server_dir = match name {
        Some(name) => get_first_server_path(name)?,
        None => get_first_server_path(format!("{platform}-server"))?,
    };

    fs::create_dir_all(&server_dir)?;
    let (jar, jar_file_name) = get_jar(download_url, platform)?;
    copy_jar(&server_dir, jar, &jar_file_name)?;
    set_default_metadata(server_dir.join(METADATA_DIRECTORY), jar_file_name)?;
    Ok(())
}

pub fn update_existing<S>(server: S, platform: Platform, version: Option<String>) -> Result<()>
where
    S: AsRef<Path>,
{
    let download_url = platforms::get(platform, version)?;
    let server_dir = get_expanded_servers_dir()?.join(&server);

    let (jar, jar_file_name) = get_jar(download_url, platform)?;
    copy_jar(&server, jar, &jar_file_name)?;
    set_jar_file_metadata(server_dir.join(METADATA_DIRECTORY), jar_file_name)?;

    Ok(())
}

pub fn save_last_used_now(server: impl AsRef<Path>) -> Result<()> {
    let now = SystemTime::now();

    let timestamp = now
        .duration_since(UNIX_EPOCH)
        .map_err(|_| Error::TimeWentBackwards)?
        .as_secs();

    set_last_used_metadata(
        get_expanded_servers_dir()?
            .join(server)
            .join(METADATA_DIRECTORY),
        timestamp,
    )?;

    Ok(())
}

pub enum LastUsed {
    Never,
    Unknown,
    Time(String),
}

pub fn get_last_used(server: impl AsRef<Path>) -> Result<LastUsed> {
    let timestamp_path = get_expanded_servers_dir()?
        .join(&server)
        .join(METADATA_DIRECTORY)
        .join(LAST_USED_FILE);

    if !timestamp_path.exists() {
        return Ok(LastUsed::Unknown);
    }

    let data = fs::read(timestamp_path)?;

    if data.len() != 8 {
        return Err(Error::InvalidTimestampFile(
            server.as_ref().to_string_lossy().to_string(),
        ));
    }

    let bytes: [u8; 8] = data
        .try_into()
        .map_err(|_| Error::InvalidTimestampFile(server.as_ref().to_string_lossy().to_string()))?;

    let timestamp = u64::from_le_bytes(bytes);

    if timestamp == u64::MAX {
        return Ok(LastUsed::Never);
    }

    let now_ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| Error::TimeWentBackwards)?
        .as_secs();

    let difference = now_ts.saturating_sub(timestamp);

    const SECS_MINUTE: u64 = 60;
    const SECS_HOUR: u64 = SECS_MINUTE * 60;
    const SECS_DAY: u64 = SECS_HOUR * 24;
    const SECS_YEAR: u64 = (SECS_DAY as f64 * 365.2425) as u64;

    let years = difference / SECS_YEAR;
    let years_remainder = difference % SECS_YEAR;

    let days = years_remainder / SECS_DAY;
    let days_remainder = years_remainder % SECS_DAY;

    let hours = days_remainder / SECS_HOUR;
    let hours_remainder = days_remainder % SECS_HOUR;

    let minutes = hours_remainder / SECS_MINUTE;
    let seconds = hours_remainder % SECS_MINUTE;

    Ok(LastUsed::Time(if years > 0 {
        format!("{years}y {days}d {hours}h {minutes}m {seconds}s")
    } else if days > 0 {
        format!("{days}d {hours}h {minutes}m {seconds}s")
    } else if hours > 0 {
        format!("{hours}h {minutes}m {seconds}s")
    } else if minutes > 0 {
        format!("{minutes}m {seconds}s")
    } else {
        format!("{seconds}s")
    }))
}

pub fn for_each(mut f: impl FnMut(String)) -> Result<()> {
    let servers_dir = get_expanded_servers_dir()?;

    if !servers_dir.exists() || !servers_dir.is_dir() {
        return Err(Error::MissingDirectory {
            dir: servers_dir.to_path_buf(),
        });
    }

    for entry in fs::read_dir(servers_dir)? {
        let entry = entry?;
        let file_name = entry.file_name().to_string_lossy().to_string();
        f(file_name);
    }

    Ok(())
}

pub fn get_all_hashed() -> Result<HashSet<String>> {
    let mut servers = HashSet::new();
    for_each(|s| {
        servers.insert(s);
    })?;
    Ok(servers)
}

pub fn get_server_dir_required(server: impl AsRef<Path>) -> Result<PathBuf> {
    let server_dir = get_expanded_servers_dir()?.join(server);

    if !server_dir.is_dir() {
        return Err(Error::MissingDirectory { dir: server_dir });
    }

    Ok(server_dir)
}

fn get_server_jar_path(server_dir: impl AsRef<Path>) -> Result<PathBuf> {
    let server_dir = server_dir.as_ref();
    let jar_file_txt = server_dir
        .join(METADATA_DIRECTORY)
        .join(JAR_FILE_TXT_NAME);

    if !jar_file_txt.is_file() {
        return Err(Error::MissingFile { file: jar_file_txt });
    }

    let jar_file_path = server_dir.join(fs::read_to_string(jar_file_txt)?.trim_end());

    if !jar_file_path.is_file() {
        return Err(Error::MissingFile {
            file: jar_file_path,
        });
    }

    Ok(jar_file_path)
}

pub fn get_command(server: impl AsRef<str>) -> Result<String> {
    let server = server.as_ref();
    if is_template(server) {
        return Err(Error::TemplateDeployed);
    }

    let server_dir = get_server_dir_required(server)?;
    let config = &config::get()?;
    Ok(format!(
        "{} action rename-tab Server && cd {} && java -jar {} {} {} && {} kill-session $ZELLIJ_SESSION_NAME",
        session::BASE_COMMAND,
        server_dir.to_string_lossy(),
        config.default_java_args,
        get_server_jar_path(&server_dir)?.to_string_lossy(),
        if config.nogui { "nogui" } else { "" },
        session::BASE_COMMAND
    ))
}

pub fn restart() -> Result<()> {
    let session_name = env::var_os("ZELLIJ_SESSION_NAME")
        .ok_or(Error::NoSessionName)?
        .to_string_lossy()
        .to_string();

    let Some(server) = session_name.strip_suffix(session::SUFFIX) else {
        return Err(Error::InvalidServerSession(session_name));
    };

    save_last_used_now(server)?;
    session::write_line(&session_name, get_command(server)?)
}

pub fn is_template(server: impl AsRef<str>) -> bool {
    server.as_ref().ends_with(TEMPLATE_SUFFIX)
}

pub fn new_template(server: impl AsRef<str>) -> Result<()> {
    let server = server.as_ref();
    if is_template(server) {
        return Err(Error::TemplateUsedForTemplate);
    }
    println!("Creating template using server {server}...");

    let servers_dir = get_expanded_servers_dir()?;

    let server_path = servers_dir.join(server);
    if !server_path.exists() {
        return Err(Error::ServerNotFound(server.to_string()));
    }

    let template_path = servers_dir.join(format!("{server}{TEMPLATE_SUFFIX}"));
    if template_path.exists() {
        return Err(Error::TemplateAlreadyExists(server.to_string()));
    }

    copy_directory(server_path, template_path)?;

    Ok(())
}

fn get_first_server_path(name: impl Display) -> Result<PathBuf> {
    let servers_dir = get_expanded_servers_dir()?;
    let path = servers_dir.join(format!("{name}"));

    if !path.exists() {
        return Ok(path);
    }

    let mut number = 2;

    Ok(loop {
        let path = servers_dir.join(format!("{name}-{number}"));
        if !path.exists() {
            break path;
        }

        number += 1;
    })
}

pub fn from_template(template: impl AsRef<str>, server: Option<impl AsRef<str>>) -> Result<()> {
    let template = template.as_ref();
    let servers_dir = get_expanded_servers_dir()?;

    let template_path = if template.ends_with(TEMPLATE_SUFFIX) {
        println!("Creating server from {template}");
        servers_dir.join(template)
    } else {
        let template_name = format!("{}{TEMPLATE_SUFFIX}", template);
        println!("Creating server from {template_name}");
        servers_dir.join(template_name)
    };

    if !template_path.exists() {
        return Err(Error::TemplateNotFound(template.to_string()));
    }

    let server_path = match server {
        Some(server) => {
            let server = server.as_ref();
            let path = get_expanded_servers_dir()?.join(server);
            if path.exists() {
                return Err(Error::ServerAlreadyExists(server.to_string()));
            }
            path
        }
        None => get_first_server_path(template)?,
    };

    copy_directory(template_path, server_path)?;

    Ok(())
}

pub fn reinstall_with_git(commit: Option<String>) -> io::Result<()> {
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

pub fn reinstall_with_path(path: impl AsRef<OsStr>) -> io::Result<()> {
    Command::new("cargo")
        .arg("install")
        .arg("--path")
        .arg(path)
        .arg("--force")
        .spawn()?
        .wait()?;

    Ok(())
}

pub fn reinstall_with_crate() -> io::Result<()> {
    Command::new("cargo")
        .arg("install")
        .arg(env!("CARGO_PKG_NAME"))
        .spawn()?
        .wait()?;

    Ok(())
}
