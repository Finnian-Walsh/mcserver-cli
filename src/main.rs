mod cli;
mod config;
mod config_defs;
mod error;
mod platforms;
mod server;
mod session;

use clap::Parser;
use cli::*;
use color_eyre::eyre::{Result, WrapErr};

fn main() -> Result<()> {
    color_eyre::install()?;

    let args = Cli::parse();

    match args.command {
        Commands::Attach { server } => session::attach(unwrap_server_or_default!(server)?)
            .wrap_err("Failed to attach to session session")?,
        Commands::Config { config_type } => match config_type {
            ConfigType::Static => println!("{:#?}", config::get_static()),
            ConfigType::Dynamic => println!("{:#?}", config::get()?),
        },
        Commands::Default { action } => match action {
            DefaultCommands::Get => {
                if let Some(default_server) = &config::get()?.default_server {
                    println!("\"{default_server}\"")
                } else {
                    println!("None")
                }
            }
            DefaultCommands::Set { server } => config::get()?.default_server = Some(server),
        },
        Commands::DeleteAllSessions { force } => if force {
            session::delete_all()
        } else {
            session::confirm_delete_all()
        }
        .wrap_err("Failed to delete all sessions")?,
        Commands::DeleteSession { session, force } => {
            session::delete_server_session(unwrap_server_or_default!(session)?, force)
                .wrap_err("Failed to delete session")?
        }
        Commands::Deploy { server } => {
            let server = unwrap_server_or_default!(server)?;
            session::new_server(&server, Some(server::get_command(&server)?))?;
        }
        Commands::Execute { server, commands } => {
            let session_name = session::get_name(unwrap_server_or_default!(server)?);
            for command in commands {
                session::write_line(&session_name, command)?;
            }
        }
        Commands::List {
            active,
            inactive,
            dead,
        } => {
            let mut servers = vec![];
            server::for_each(|s| servers.push(server::ServerObject::new(s)))
                .wrap_err("Failed to get servers")?;

            if active {
                server::retain_active(&mut servers).wrap_err("Failed to retain active servers")?;
            } else if inactive {
                server::retain_and_tag_inactive(&mut servers)
                    .wrap_err("Failed to retain inactive servers")?;
                if dead {
                    server::tag_dead(&mut servers).wrap_err("Failed to tag dead servers")?;
                }
            } else if dead {
                server::retain_and_tag_dead(&mut servers)
                    .wrap_err("Failed to retain dead servers")?;
            } else {
                server::fully_tag_servers(&mut servers).wrap_err("Failed to tag active servers")?;
            }

            for server in servers {
                println!("{server}");
            }
        }
        Commands::Rcon { server, commands } => {
            server::rcon(unwrap_server_or_default!(server)?, commands)
                .wrap_err("Failed to run rcon command")?
        }
        Commands::New {
            platform,
            version,
            name,
        } => server::create_new(platform, version, name)
            .wrap_err(format!("Failed to create {platform} server"))?,
        Commands::Remove { servers, force } => if force {
            server::remove_servers(servers)
        } else {
            server::remove_servers_with_confirmation(servers)
        }
        .wrap_err("Failed to remove server")?,
        Commands::Restart => server::restart().wrap_err("Failed to restart server")?,
        Commands::Stop { server } => {
            let server = unwrap_server_or_default!(server)?;
            server::rcon(&server, vec!["stop"])
                .wrap_err_with(|| format!("Failed to stop server {}", &server))?;
        }
        Commands::Template { action } => match action {
            TemplateCommands::New { server } => server::new_template(&server)
                .wrap_err_with(|| format!("Failed to create template with server {server}"))?,
            TemplateCommands::From { template, server } => {
                server::from_template(&template, server.as_deref())
                    .wrap_err_with(|| format!("Failed to use template {template}"))?
            }
        },
        Commands::Reinstall {
            git,
            commit,
            path,
            from_crate,
        } => {
            if let Some(path) = path {
                server::reinstall_with_path(&path)
                    .wrap_err(format!("Failed to update package with {}", path.display()))?
            } else if git {
                server::reinstall_with_git(commit)
                    .wrap_err("Failed to update package with git repo")?
            } else if from_crate {
                server::reinstall_with_crate().wrap_err("Failed to update package with crate")?
            } else {
                unreachable!("Clap ensures git or a path is provided")
            }
        }
        Commands::Update {
            server,
            platform,
            version,
        } => server::update_existing(server, platform, version)
            .wrap_err("Failed to update server")?,
    };

    config::CONFIG.write()?;

    Ok(())
}
