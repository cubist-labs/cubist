use crate::cube::{git::GitUrl, template::Template};
use crate::daemon::{DaemonFilter, StartArgs, StartCommand};
use clap::{Parser, Subcommand};
use cubist_config::ProjType;
use std::fmt::Debug;
use std::path::PathBuf;

pub const BINARY_NAME: &str = "cubist";

#[derive(Debug, Parser)]
#[clap(name = BINARY_NAME, about = "Multi-chain Web3 development and deployment framework", long_about = None)]
pub struct Cli {
    #[clap(subcommand)]
    pub command: Commands,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
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
