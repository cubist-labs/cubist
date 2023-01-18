#![doc(html_no_source)]
use clap::{CommandFactory, Parser, Subcommand};
use clap_complete::{generate_to, Shell};
use clap_mangen::Man;
use color_eyre::eyre::Result;
use cubist_cli::cli::{Cli as CubistCli, BINARY_NAME};

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

mod generate_markdown;
mod generate_schema;
mod populate_hashes;

#[derive(Debug, Parser)]
#[clap(about = "xtasks for the Cubist SDK", long_about = None)]
pub struct Cli {
    #[clap(subcommand)]
    pub command: Commands,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    /// Produce auto completion files for the Cubist CLI
    GenerateAutoComplete,
    /// Produce documentation for the Cubist CLI
    GenerateCliDocs,
    /// Produce man pages for the Cubist CLI
    GenerateMan,
    /// Generate JSON schema files
    GenerateSchema {
        /// Output directory
        #[clap(value_parser)]
        out: PathBuf,
    },
    /// Populate expected hashes for downloads
    PopulateHashes,
}

fn root_path() -> PathBuf {
    let xtask_path = env!("CARGO_MANIFEST_DIR").to_string();
    Path::new(&xtask_path)
        .parent()
        .expect("Root directory exists")
        .into()
}

#[tokio::main]
async fn main() -> Result<()> {
    color_eyre::install()?;

    let args = Cli::parse();
    match args.command {
        Commands::GenerateAutoComplete => {
            let complete_path = root_path().join("complete");
            fs::create_dir_all(&complete_path)?;
            for shell in [Shell::Bash, Shell::Fish, Shell::Zsh] {
                generate_to(
                    shell,
                    &mut CubistCli::command(), // We need to specify what generator to use
                    BINARY_NAME,               // We need to specify the bin name manually
                    &complete_path,
                )?;
            }
        }
        Commands::GenerateCliDocs => {
            generate_markdown::run(&mut io::stdout(), &CubistCli::command())?;
        }
        Commands::GenerateMan => {
            let man_path = root_path().join("man");
            fs::create_dir_all(&man_path)?;

            let man = Man::new(CubistCli::command());
            let mut buffer: Vec<u8> = Default::default();
            man.render(&mut buffer)?;
            std::fs::write(man_path.join(format!("{}.1", BINARY_NAME)), buffer)?;
        }
        Commands::GenerateSchema { out } => {
            generate_schema::run(out)?;
        }
        Commands::PopulateHashes => {
            populate_hashes::run().await?;
        }
    }

    Ok(())
}
