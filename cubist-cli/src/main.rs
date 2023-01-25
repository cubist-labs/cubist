use clap::Parser;
use color_eyre::eyre::{eyre, Result, WrapErr};
use console::style;
use cubist_cli::cli::{Cli, Commands};
use cubist_cli::commands::{compile::compile, gen, new, pre_compile::pre_compile};
use cubist_cli::daemon::{DaemonManager, StartCommand};
use cubist_config::Config;

use std::env;
use std::path::PathBuf;

#[tokio::main]
async fn main() -> Result<()> {
    color_eyre::install()?;
    tracing_subscriber::fmt::init();
    let args = Cli::parse();

    let load_config = |path: &Option<PathBuf>| {
        match path {
            None => Config::nearest(),
            Some(file) => Config::from_file(file),
        }
        .wrap_err("Could not load config")
    };

    match args.command {
        Commands::New {
            name,
            type_,
            template,
            from_repo,
            dir,
            force,
            branch,
        } => {
            let dir = dir.or_else(|| env::current_dir().ok()).unwrap();
            if let Some(url) = from_repo {
                new::from_git_repo(&name, &url, &dir, force)?;
            } else if let Some(template) = template {
                new::from_template(&name, type_, template, &dir, force, branch)?
            } else {
                new::empty(&name, type_, &dir, force)?;
            }
        }
        Commands::PreCompile { config } => {
            let cfg = load_config(&config)?;
            pre_compile(&cfg)?;
        }
        Commands::Compile { config } => {
            let cfg = load_config(&config)?;
            compile(&cfg)?;
        }
        Commands::Build { config } => {
            let cfg = load_config(&config)?;
            pre_compile(&cfg)?;
            compile(&cfg)?;
            gen::gen_orm(cfg)?;
        }
        Commands::Gen { config } => {
            let cfg = load_config(&config)?;
            gen::gen_orm(cfg)?;
        }
        Commands::Start {
            config,
            args,
            command,
        } => {
            use StartCommand::{Chains, Relayer};
            let cfg = load_config(&config)?;
            let daemonize = args.daemonize();
            match command {
                Some(cmd) => DaemonManager::start(cfg, args, cmd, false).await?,
                None => {
                    let run_relayer = cfg.contracts().targets.len() > 1;
                    let force_in_bg = run_relayer;
                    DaemonManager::start(cfg.clone(), args.clone(), Chains, force_in_bg).await?;
                    if run_relayer {
                        DaemonManager::start(cfg, args, Relayer(Default::default()), false).await?;
                    }
                }
            };
            // return now if daemonizing to avoid "Done!" being printed out
            if daemonize {
                return Ok(());
            }
        }
        Commands::Stop(filter) => {
            let filter = filter.canonicalize();
            DaemonManager::stop(&filter)?;
            return Ok(());
        }
        Commands::Status { filter, json } => {
            let filter = filter.canonicalize();
            let num_running = DaemonManager::status(&filter, json).await?;
            if !json && num_running == 0 {
                return Err(eyre!("No running 'cubist' daemon found"));
            } else {
                return Ok(());
            }
        }
    }

    println!("{}", style("Done!").bold().green());
    Ok(())
}
