use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    ffi::OsStr,
    fmt::{self, Debug, Formatter},
};

pub trait AllowedConfigValue {}
impl AllowedConfigValue for String {}
impl AllowedConfigValue for &'static str {}

#[derive(Debug, Deserialize)]
pub struct StaticConfig<T = &'static str>
where
    T: AllowedConfigValue,
{
    pub contact: T,
    pub dynamic_config_path: T,
}

#[derive(Clone, Deserialize, PartialEq, Serialize)]
pub struct Password(pub String);

impl AsRef<OsStr> for Password {
    fn as_ref(&self) -> &OsStr {
        OsStr::new(&self.0)
    }
}

impl Debug for Password {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "(hidden)")
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct RconConfig {
    pub server_address: Option<String>,
    pub port: Option<u16>,
    pub password: Option<Password>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct DynamicConfig {
    pub default_java_args: String,
    pub nogui: bool,
    pub servers_directory: String,
    pub default_server: Option<String>,
    pub rcon: Option<HashMap<String, RconConfig>>,
}
