use clap::{Parser, Subcommand};
use color_eyre::eyre::{eyre, Result, WrapErr};
use console::style;
use cubist_cli::commands::{compile::compile, gen, new, pre_compile::pre_compile};
use cubist_cli::cube::{git::GitUrl, template::Template};
use cubist_cli::daemon::{DaemonFilter, DaemonManager, StartArgs, StartCommand};
use cubist_config::{Config, ProjType};

use std::env;
use std::fmt::Debug;
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[clap(about = "Multi-chain Web3 development and deployment framework", long_about = None)]
struct Cli {
    #[clap(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Create new empty project
    #[clap(arg_required_else_help = true)]
    New {
        /// Project name
        #[clap(value_parser)]
        name: String,
        /// Project type
        #[clap(short = 't', long = "type", value_name = "TYPE")]
        #[clap(name = "type", value_enum, default_value = "TypeScript")]
        type_: ProjType,
        /// Create project from template
        #[clap(long = "template", value_parser, value_name = "TEMPLATE")]
        #[clap(value_enum)]
        template: Option<Template>,
        /// Create project from git repo template
        #[clap(
            long = "from-repo", value_name = "GIT_URL",
            conflicts_with_all = &["type", "template"]
        )]
        from_repo: Option<GitUrl>,
        /// Directory where to create project
        #[clap(long = "dir", value_parser, value_hint = clap::ValueHint::DirPath)]
        dir: Option<PathBuf>,
        /// Force creation (e.g., by overwrite existing files or ignoring non-standard templates)
        #[clap(long, action, default_value = "false")]
        force: bool,
        /// Branch to pull from, if creating a project from template
        #[clap(long, value_parser, requires = "template")]
        branch: Option<String>,
    },
    /// Generate contract interfaces
    PreCompile {
        /// Explicit config file
        #[clap(short = 'c', long = "config", value_parser, value_hint = clap::ValueHint::FilePath)]
        config: Option<PathBuf>,
    },
    /// Compile contracts
    Compile {
        /// Explicit config file
        #[clap(short = 'c', long = "config", value_parser, value_hint = clap::ValueHint::FilePath)]
        config: Option<PathBuf>,
    },
    /// Build (pre-compile + compile + gen)
    Build {
        /// Explicit config file
        #[clap(short = 'c', long = "config", value_parser, value_hint = clap::ValueHint::FilePath)]
        config: Option<PathBuf>,
    },
    /// Generate code (for now, defaults to ORM, later will have other options)
    Gen {
        /// Explicit config file
        #[clap(short = 'c', long = "config", value_parser, value_hint = clap::ValueHint::FilePath)]
        config: Option<PathBuf>,
    },
    /// Start a Cubist service (e.g., chains or relayer)
    Start {
        /// Explicit config file
        #[clap(short = 'c', long = "config", value_parser, value_hint = clap::ValueHint::FilePath)]
        config: Option<PathBuf>,
        #[clap(flatten)]
        args: StartArgs,
        /// When omitted, start everything using default settings
        #[clap(subcommand)]
        command: Option<StartCommand>,
    },
    /// Stop a running Cubist service
    Stop(DaemonFilter),
    /// Print out the status of running Cubist services
    Status {
        #[clap(flatten)]
        filter: DaemonFilter,
        #[clap(short = 'j', long = "json")]
        json: bool,
    },
}

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
