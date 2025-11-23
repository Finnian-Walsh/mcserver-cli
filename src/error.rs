use reqwest::header;
use std::{
    env::VarError,
    io,
    path::{self, PathBuf},
    result,
};
use thiserror::Error;

#[non_exhaustive]
#[derive(Debug, Error)]
pub enum Error {
    #[error(
        "Command failed with code {}{}",
        code.map(|c| c.to_string()).as_deref().unwrap_or("none"),
        stderr
            .as_ref()
            .map(|err| format!(": {}", String::from_utf8_lossy(err)))
            .as_deref()
            .unwrap_or("")
    )]
    CommandFailure {
        code: Option<i32>,
        stderr: Option<Vec<u8>>,
    },

    #[error(transparent)]
    InvalidHeaderValue(#[from] header::InvalidHeaderValue),

    #[error("Invalid server session: `{0}`")]
    InvalidServerSession(String),

    #[error("Invalid servers directory")]
    InvalidServersDirectory,

    #[error("Timestamp file ({0}) is invalid")]
    InvalidTimestampFile(String),

    #[error(transparent)]
    Io(#[from] io::Error),

    #[error("Missing directory: {}", dir.display())]
    MissingDirectory { dir: PathBuf },

    #[error("Missing file: {}", file.display())]
    MissingFile { file: PathBuf },

    #[error("There is no default server")]
    NoDefaultServer,

    #[error("Rcon config is not present, but required for remote connections")]
    NoRconConfig,

    #[error("No server child was given")]
    NoServerChild,

    #[error("No session name found")]
    NoSessionName,

    #[error("Platforms not found: {0}")]
    PlatformsNotFound(String),

    #[error("The configuration mutex has been poisoned")]
    ConfigMutexPoisoned,

    #[error("Rcon config is missing for server: {0}")]
    MissingRconConfig(String),

    #[error(transparent)]
    Reqwest(#[from] reqwest::Error),

    #[error(transparent)]
    ShellexpandLookup(#[from] shellexpand::LookupError<VarError>),

    #[error("Server {0} already exists")]
    ServerAlreadyExists(String),

    #[error("The machine's local time went backwards")]
    TimeWentBackwards,

    #[error("Server {0} was not found")]
    ServerNotFound(String),

    #[error(transparent)]
    StripPrefix(#[from] path::StripPrefixError),

    #[error("Template {0} already exists")]
    TemplateAlreadyExists(String),

    #[error("Template servers cannot be deployed")]
    TemplateDeployed,

    #[error("Template with the name {0} was not found")]
    TemplateNotFound(String),

    #[error("Cannot create a template with a template")]
    TemplateUsedForTemplate,

    #[error(transparent)]
    TomlDeserialize(#[from] toml::de::Error),

    #[error(transparent)]
    TomlSerialize(#[from] toml::ser::Error),

    #[error(transparent)]
    ToStr(#[from] header::ToStrError),

    #[error(transparent)]
    UrlParse(#[from] url::ParseError),
}

pub type Result<T> = result::Result<T, Error>;
