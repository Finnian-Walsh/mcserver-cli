use crate::platforms::Platform;
use clap::{ArgGroup, Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "mcserver", version, about = "Minecraft server CLI tool")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[non_exhaustive]
#[derive(Subcommand)]
pub enum Commands {
    #[command(visible_alias = "a", about = "Attach to a server session")]
    Attach { server: Option<String> },

    #[command(visible_alias = "cfg", about = "Query the configuration")]
    Config {
        #[command(subcommand)]
        config_type: ConfigType,
    },

    #[command(visible_alias = "def", about = "Set or get the default server")]
    Default {
        #[command(subcommand)]
        action: DefaultCommands,
    },

    #[command(
        subcommand = "delete-all-sessions",
        visible_alias = "da",
        about = "Safely delete all server dead server sessions"
    )]
    DeleteAllSessions {
        #[arg(short, long)]
        force: bool,
    },

    #[command(
        subcommand = "delete-session",
        visible_alias = "d",
        about = "Safely delete the session for a server (must be dead)"
    )]
    DeleteSession {
        #[arg(short, long)]
        force: bool,

        session: Option<String>,
    },

    #[command(visible_alias = "dpl", about = "Deploy a server")]
    Deploy { server: Option<String> },

    #[command(visible_alias = "exec", about = "Execute a command on a server")]
    Execute {
        #[arg(short, long)]
        server: Option<String>,

        #[arg(trailing_var_arg = true)]
        commands: Vec<String>,
    },

    #[command(visible_alias = "ls", about = "List all, active or inactive servers")]
    List {
        #[arg(short, long, conflicts_with_all = ["inactive", "dead"])]
        active: bool,

        #[arg(short, long, conflicts_with = "active")]
        inactive: bool,

        #[arg(short, long, conflicts_with = "inactive")]
        dead: bool,
    },

    #[command(about = "Interact with a server, using the minecraft remote console")]
    Rcon {
        server: Option<String>,

        commands: Vec<String>,
    },

    #[command(about = "Create a new server")]
    New {
        #[clap(value_enum)]
        platform: Platform,

        #[arg(short, long)]
        name: Option<String>,

        #[arg(short, long)]
        version: Option<String>,
    },

    #[command(visible_alias = "reinst", about = "Reinstall the server binary",
        group(
                ArgGroup::new("source")
                    .args(&["git", "path", "from_crate"])
                    .required(true)
            )
    )]
    Reinstall {
        #[arg(short = 'c', long = "crate")]
        from_crate: bool,

        #[arg(short, long)]
        git: bool,

        #[arg(long, requires = "git")]
        commit: Option<String>,

        #[arg(short, long)]
        path: Option<PathBuf>,
    },

    #[command(visible_alias = "rm", about = "Remove a server")]
    Remove {
        #[arg(short, long)]
        force: bool,

        servers: Vec<String>,
    },

    #[command(visible_alias = "rst", about = "Restart the current server")]
    Restart,

    #[command(about = "Stop a server")]
    Stop { server: Option<String> },

    #[command(visible_alias = "tmpl", about = "Create or use a template server")]
    Template {
        #[command(subcommand)]
        action: TemplateCommands,
    },

    #[command(about = "Update a server's .jar file and reference")]
    Update {
        server: String,

        platform: Platform,

        version: Option<String>,
    },
}

#[derive(Subcommand)]
pub enum ConfigType {
    Static,

    #[clap(visible_alias = "dyn")]
    Dynamic,
}

#[derive(Subcommand)]
pub enum DefaultCommands {
    Get,

    Set { server: String },
}

#[derive(Subcommand)]
pub enum TemplateCommands {
    New {
        server: String,
    },

    From {
        template: String,

        #[arg(short, long)]
        server: Option<String>,
    },
}
