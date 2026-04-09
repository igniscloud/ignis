use std::path::PathBuf;

use clap::{Parser, Subcommand, ValueEnum};

#[derive(Debug, Parser)]
#[command(name = "ignis", version, about = "Ignis project and service CLI")]
pub struct Cli {
    #[arg(
        long,
        global = true,
        value_name = "TOKEN",
        help = "Project token, login token, or API token for igniscloud; also supports IGNIS_TOKEN or IGNISCLOUD_TOKEN"
    )]
    pub token: Option<String>,
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    Login,
    Logout,
    Whoami,
    GenSkill {
        #[arg(long, value_enum, default_value_t = SkillFormat::Codex)]
        format: SkillFormat,
        #[arg(long)]
        path: Option<PathBuf>,
        #[arg(long)]
        force: bool,
    },
    Project {
        #[command(subcommand)]
        command: ProjectCommands,
    },
    Service {
        #[command(subcommand)]
        command: ServiceCommands,
    },
    #[command(hide = true)]
    Internal {
        #[command(subcommand)]
        command: InternalCommands,
    },
}

#[derive(Debug, Subcommand)]
pub enum ProjectCommands {
    Create {
        name: String,
        #[arg(long)]
        dir: Option<PathBuf>,
        #[arg(long)]
        force: bool,
    },
    Sync,
    List,
    Status {
        project: String,
    },
    Delete {
        project: String,
    },
    Token {
        #[command(subcommand)]
        command: ProjectTokenCommands,
    },
}

#[derive(Debug, Subcommand)]
pub enum ProjectTokenCommands {
    Create {
        project: String,
        #[arg(long)]
        issued_for: Option<String>,
    },
    Revoke {
        project: String,
        token_id: String,
    },
}

#[derive(Debug, Clone, Copy, ValueEnum, PartialEq, Eq)]
pub enum SkillFormat {
    Codex,
    Opencode,
    Raw,
}

#[derive(Debug, Subcommand)]
pub enum ServiceCommands {
    New {
        #[arg(long)]
        service: String,
        #[arg(long)]
        kind: CliServiceKind,
        #[arg(long)]
        path: PathBuf,
    },
    List,
    Status {
        #[arg(long)]
        service: String,
    },
    Check {
        #[arg(long)]
        service: String,
    },
    Delete {
        #[arg(long)]
        service: String,
    },
    Build {
        #[arg(long)]
        service: String,
        #[arg(long, default_value_t = true)]
        release: bool,
    },
    Publish {
        #[arg(long)]
        service: String,
    },
    Deploy {
        #[arg(long)]
        service: String,
        version: String,
    },
    Deployments {
        #[arg(long)]
        service: String,
        #[arg(long, default_value_t = 100)]
        limit: u32,
    },
    Events {
        #[arg(long)]
        service: String,
        #[arg(long, default_value_t = 100)]
        limit: u32,
    },
    Logs {
        #[arg(long)]
        service: String,
        #[arg(long, default_value_t = 100)]
        limit: u32,
    },
    Rollback {
        #[arg(long)]
        service: String,
        version: String,
    },
    DeleteVersion {
        #[arg(long)]
        service: String,
        version: String,
    },
    Env {
        #[command(subcommand)]
        command: ServiceEnvCommands,
    },
    Secrets {
        #[command(subcommand)]
        command: ServiceSecretCommands,
    },
    Sqlite {
        #[command(subcommand)]
        command: ServiceSqliteCommands,
    },
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum CliServiceKind {
    Http,
    Frontend,
}

#[derive(Debug, Subcommand)]
pub enum ServiceEnvCommands {
    List {
        #[arg(long)]
        service: String,
    },
    Set {
        #[arg(long)]
        service: String,
        name: String,
        value: String,
    },
    Delete {
        #[arg(long)]
        service: String,
        name: String,
    },
}

#[derive(Debug, Subcommand)]
pub enum ServiceSecretCommands {
    List {
        #[arg(long)]
        service: String,
    },
    Set {
        #[arg(long)]
        service: String,
        name: String,
        value: String,
    },
    Delete {
        #[arg(long)]
        service: String,
        name: String,
    },
}

#[derive(Debug, Subcommand)]
pub enum ServiceSqliteCommands {
    Backup {
        #[arg(long)]
        service: String,
        out: PathBuf,
    },
    Restore {
        #[arg(long)]
        service: String,
        input: PathBuf,
    },
}

#[derive(Debug, Subcommand)]
pub enum InternalCommands {
    CopyFrontendStatic {
        #[arg(long, default_value = "src")]
        source_dir: PathBuf,
        #[arg(long, default_value = "dist")]
        output_dir: PathBuf,
    },
}
