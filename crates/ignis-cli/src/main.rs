mod api;
mod auth;
mod build;
mod cli;
mod config;
mod context;
mod output;
mod project;
mod service;
mod skill;
mod skill_bundle;
mod template;

use anyhow::Result;
use clap::Parser;
use tracing_subscriber::EnvFilter;

use crate::cli::{Cli, Commands};

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .without_time()
        .init();

    if let Err(error) = run().await {
        if let Err(render_error) = output::failure(&error) {
            eprintln!("{error:#}");
            eprintln!("failed to render CLI error JSON: {render_error}");
        }
        std::process::exit(1);
    }
}

async fn run() -> Result<()> {
    let Cli { token, command } = Cli::parse();
    match command {
        Commands::Login => auth::login(token).await,
        Commands::Logout => auth::logout(),
        Commands::Whoami => auth::whoami(token).await,
        Commands::GenSkill {
            format,
            path,
            force,
        } => skill::generate(format, path.as_deref(), force),
        Commands::Project { command } => project::handle(command, token).await,
        Commands::Service { command } => service::handle(command, token).await,
    }
}
