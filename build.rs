use color_eyre::eyre::{Result, WrapErr, eyre};
use quote::quote;
use serde::Deserialize;
use std::{
    env, fs,
    path::{Path, PathBuf},
};

mod config_defs {
    include!("src/config_defs.rs");

    use proc_macro2::TokenStream;
    use quote::{ToTokens, quote};

    impl ToTokens for StaticConfig<String> {
        fn to_tokens(&self, tokens: &mut TokenStream) {
            let contact = &self.contact;
            let dynamic_config_path = &self.dynamic_config_path;
            tokens.extend(quote! {
                StaticConfig {
                    contact: #contact,
                    dynamic_config_path: #dynamic_config_path,
                }
            });
        }
    }

    impl ToTokens for Password {
        fn to_tokens(&self, tokens: &mut TokenStream) {
            let password = &self.0;
            tokens.extend(quote! { Password { #password } })
        }
    }

    impl ToTokens for RconConfig {
        fn to_tokens(&self, tokens: &mut TokenStream) {
            let server_address = match self.server_address.as_ref() {
                Some(server_address) => quote! {
                    Some(#server_address.to_string())
                },
                None => quote! { None },
            };

            let port = match self.port {
                Some(port) => quote! { Some(#port) },
                None => quote! { None },
            };

            let password = match self.password.as_ref() {
                Some(password) => quote! { Some(#password) },
                None => quote! { None },
            };

            tokens.extend(quote! {
                RconConfig {
                    server_address: #server_address,
                    port: #port,
                    password: #password,
                }
            })
        }
    }

    impl ToTokens for DynamicConfig {
        fn to_tokens(&self, tokens: &mut TokenStream) {
            let default_java_args = &self.default_java_args;
            let nogui = &self.nogui;
            let servers_directory = &self.servers_directory;

            let default_server_quote = if let Some(default_server) = &self.default_server {
                quote! { Some(#default_server.to_string()) }
            } else {
                quote! { None }
            };

            let rcon_quote = if let Some(rcon) = &self.rcon {
                let key_value_pairs = rcon.iter().map(|(k, v)| {
                    quote! { ( #k.to_string(), #v )}
                });

                quote! {
                    Some(std::collections::HashMap::from([
                        #(#key_value_pairs),*
                    ]))
                }
            } else {
                quote! { None }
            };

            tokens.extend(quote! {
                DynamicConfig {
                    default_java_args: #default_java_args.to_string(),
                    nogui: #nogui,
                    servers_directory: #servers_directory.to_string(),
                    default_server: #default_server_quote,
                    rcon: #rcon_quote,
                }
            });
        }
    }
}

use config_defs::{DynamicConfig, StaticConfig};

#[derive(Debug, Deserialize)]
struct Config {
    static_config: StaticConfig<String>,
    default_dynamic_config: DynamicConfig,
}

macro_rules! warning {
    ($($arg:tt)*) => {
        println!("cargo:warning={}", format!($($arg)*))
    }
}

macro_rules! build_log {
    ($($arg:tt)*) => {
        #[cfg(feature = "build-logging")]
        warning!($($arg)*)
    }
}

fn handle_static_config(cargo_manifest_dir: &Path) -> Result<Option<StaticConfig<String>>> {
    let static_config_path = cargo_manifest_dir.join("static_config.toml");

    if !static_config_path.exists() {
        build_log!("The static configuration file does not exist");
        return Ok(None);
    }

    build_log!("Static configuration file found");

    if !static_config_path.is_file() {
        warning!("The static configuration given is not a file");
        return Ok(None);
    }

    build_log!("Static configuration is a file");

    let result = toml::from_str(
        &fs::read_to_string(static_config_path).wrap_err("Failed to read configuration file")?,
    );

    match result {
        Ok(static_config) => {
            build_log!("Static configuration read");
            Ok(static_config)
        }
        Err(err) => {
            warning!("Failed to parse static configuration: {err}");
            Ok(None)
        }
    }
}

fn main() -> Result<()> {
    build_log!("Build script running...");
    println!("cargo:rerun-if-changed=");

    let out_dir = PathBuf::new().join(env::var("OUT_DIR")?);
    build_log!("Out directory: {out_dir:?}");

    let cargo_manifest_dir = PathBuf::new().join(env::var("CARGO_MANIFEST_DIR")?);
    build_log!("Cargo manifest dir: {cargo_manifest_dir:?}");

    let cfg_generation_file = &out_dir.join("generated_cfg.rs");
    let config_template_path = &cargo_manifest_dir.join("config_template.toml");

    if !config_template_path.exists() {
        build_log!("Config path ({config_template_path:?}) does not exist");
        return Err(eyre!("Configuration template does not exist"));
    }

    build_log!("Configuration path exists ({config_template_path:?})");

    if !config_template_path.is_file() {
        build_log!("Configuration template should be a file",);
        return Err(eyre!("Invalid configuration template"));
    }

    let config: Config = toml::from_str(
        &fs::read_to_string(config_template_path).wrap_err("Failed to read configuration file")?,
    )
    .wrap_err("Failed to parse configuration file")?;

    let static_config = match handle_static_config(&cargo_manifest_dir)? {
        Some(static_config) => {
            build_log!("Using static configuration");
            static_config
        }
        None => {
            build_log!("Static configuration is not being used");
            config.static_config
        }
    };

    let default_dynamic_config = config.default_dynamic_config;

    let tokens = quote! {
        mod generated_cfg {
            use crate::config_defs::{StaticConfig, DynamicConfig};
            #[allow(unused)]
            use crate::config_defs::{RconConfig, Password};
            use std::sync::OnceLock;

            pub const STATIC_CONFIG: StaticConfig = #static_config;
            pub static DEFAULT_DYNAMIC_CONFIG: OnceLock<DynamicConfig> = OnceLock::new();

            pub fn get_default_dynamic_config() -> &'static DynamicConfig {
                DEFAULT_DYNAMIC_CONFIG.get_or_init(||
                    #default_dynamic_config
                )
            }
        }
    };

    fs::write(cfg_generation_file, tokens.to_string())?;

    let expanded_dynamic_config_dir = shellexpand::full(&static_config.dynamic_config_path)?;
    let dynamic_config_template_path = Path::new(&*expanded_dynamic_config_dir).join("config.toml");

    if dynamic_config_template_path.exists() {
        if !dynamic_config_template_path.is_file() {
            build_log!(
                "There is something at the path where the dynamic configuration is supposed to exist; this will cause problems in the future"
            );
        } else {
            build_log!("Dynamic configuration found");
        }
    } else {
        fs::create_dir_all(expanded_dynamic_config_dir.to_string())?;
        fs::write(
            &dynamic_config_template_path,
            toml::to_string(&default_dynamic_config)
                .wrap_err("Failed to serialize dynamic configuration")?,
        )
        .wrap_err_with(|| {
            format!("Failed to write to the dynamic configuration path ({dynamic_config_template_path:?})")
        })?;
    }

    build_log!("Configuration has been generated");

    Ok(())
}
