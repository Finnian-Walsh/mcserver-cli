use crate::{
    config::{self, get_expanded_servers_dir, server_or_current},
    error::{Error, Result},
    platforms::Platform,
    session,
};
use reqwest::{blocking, header};
use std::{
    collections::HashSet,
    env,
    fmt::{self, Display, Formatter},
    fs::{self, File},
    io::{self, Write},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};
use url::Url;

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

fn copy_jar(server_dir: impl AsRef<Path>, file_name: String, mut jar: impl io::Read) -> Result<()> {
    env::set_current_dir(server_dir)?;

    let mut jar_file = File::create(&file_name)?;
    io::copy(&mut jar, &mut jar_file)?;

    let mut jar_file_txt = File::create("jar_file.txt")?;
    writeln!(jar_file_txt, "{file_name}")?;

    Ok(())
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

pub fn init(download_url: Url, platform: Platform, name: Option<String>) -> Result<()> {
    let name = name.unwrap_or_else(|| format!("{platform}-server"));
    let servers_dir = &get_expanded_servers_dir()?;

    let mut server_dir = servers_dir.join(&name);

    if server_dir.exists() {
        let mut number = 2;

        server_dir = loop {
            let dir = servers_dir.join(format!("{}-{}", &name, number));

            if !dir.exists() {
                break dir;
            }
            number += 1;
        }
    }

    fs::create_dir_all(&server_dir)?;

    reinit(download_url, server_dir, platform)
}

pub fn reinit(download_url: Url, server_dir: impl AsRef<Path>, platform: Platform) -> Result<()> {
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

    if let Err(err) = copy_jar(&server_dir, file_name, response) {
        remove_dir_with_retries(server_dir)?;
        return Err(err);
    }

    Ok(())
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

const LAST_USED_FILE: &str = "last_used.timestamp";

pub fn get_last_used(server: impl AsRef<Path>) -> Result<Option<String>> {
    let timestamp_path = get_expanded_servers_dir()?
        .join(&server)
        .join(LAST_USED_FILE);

    if !timestamp_path.exists() {
        return Ok(None);
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

    if years > 0 {
        Ok(Some(format!(
            "{years}y {days}d {hours}h {minutes}m {seconds}s"
        )))
    } else if days > 0 {
        Ok(Some(format!("{days}d {hours}h {minutes}m {seconds}s")))
    } else if hours > 0 {
        Ok(Some(format!("{hours}h {minutes}m {seconds}s")))
    } else if minutes > 0 {
        Ok(Some(format!("{minutes}m {seconds}s")))
    } else {
        Ok(Some(format!("{seconds}s")))
    }
}

pub fn save_last_used(server: impl AsRef<Path>) -> Result<()> {
    let now = SystemTime::now();

    let timestamp = now
        .duration_since(UNIX_EPOCH)
        .map_err(|_| Error::TimeWentBackwards)?
        .as_secs();

    let timestamp_path = get_expanded_servers_dir()?
        .join(&server)
        .join(LAST_USED_FILE);
    let mut file = File::create(timestamp_path)?;
    file.write_all(&timestamp.to_le_bytes())?;

    Ok(())
}

pub fn get_server_dir_required(server: &str) -> Result<PathBuf> {
    let server_dir = get_expanded_servers_dir()?.join(server);

    if !server_dir.is_dir() {
        return Err(Error::MissingDirectory { dir: server_dir });
    }

    Ok(server_dir)
}

fn get_server_jar_path(server_dir: &Path) -> Result<PathBuf> {
    let jar_file_txt = server_dir.join("jar_file.txt");

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

pub fn get_command(server: &str) -> Result<String> {
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

    save_last_used(server)?;
    session::write_line(&session_name, get_command(server)?)
}

const TEMPLATE_SUFFIX: &str = ".template";

pub fn is_template(server: &str) -> bool {
    server.ends_with(TEMPLATE_SUFFIX)
}

pub fn new_template(server: &str) -> Result<()> {
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

fn get_first_server_path(servers_dir: impl AsRef<Path>, name: &str) -> PathBuf {
    let path = servers_dir.as_ref().join(name);
    if !path.exists() {
        return path;
    }

    let mut number = 2;
    loop {
        let path = servers_dir.as_ref().join(format!("{name}-{number}"));
        if !path.exists() {
            break path;
        }

        number += 1;
    }
}

pub fn from_template(template: &str, server: Option<&str>) -> Result<()> {
    let servers_dir = get_expanded_servers_dir()?;

    let template_path = if template.ends_with(TEMPLATE_SUFFIX) {
        println!("Creating server from {template}");
        servers_dir.join(template)
    } else {
        let template_name = format!("{template}{TEMPLATE_SUFFIX}");
        println!("Creating server from {template_name}");
        servers_dir.join(template_name)
    };

    if !template_path.exists() {
        return Err(Error::TemplateNotFound(template.to_string()));
    }

    let server_path = match server {
        Some(server) => {
            let path = get_expanded_servers_dir()?.join(server);
            if path.exists() {
                return Err(Error::ServerAlreadyExists(server.to_string()));
            }
            path
        }
        None => get_first_server_path(servers_dir, template),
    };

    copy_directory(template_path, server_path)?;

    Ok(())
}
