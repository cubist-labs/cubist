use std::path::PathBuf;

use console::style;
use cubist_config::Target;
use cubist_sdk::core::TargetProjectInfo;
use eyre::{bail, Result};

use cubist_config::{Compiler, Config};

/// Command that compiles contracts.
///
/// Each contract is compiled with solc or solang (depending on the configuration). The
/// output is then written in a compiler-specific artifacts directory. Here is an example of the
/// directory structure post (transpilation and then post) compilation.
///
/// ```text
///   build
///   ├── target-1 (e.g., avalanche)
///   │   ├── artifacts
///   │   │   ├── FileName1.sol
///   │   │   │   └── Contract1.json
///   │   │   └── FileName2.sol
///   │   │       └── Contract2.json
///   │   ├── build_infos
///   │   │   ├── <id-of-Contract1>.json
///   │   │   └── <id-of-Contract2>.json
///   │   ├── cache
///   │   └── contracts
///   │       ├── Contract2.bridge.json
///   │       ├── FileName1.sol (original)
///   │       └── FileName2.sol (generated interface)
///   └── target-2 (e.g., ethereum)
///       ├── artifacts
///       │   └── FileName2.sol
///       │       └── Contract2.json
///       ├── build_infos
///       │   └── <id-of-Contract2>.json
///       ├── cache
///       └── contracts
///           └── FileName2.sol (original)
/// ```
pub fn compile(config: &Config) -> Result<()> {
    compile_solc_files(config)
}

/// Compile contracts with solc
fn compile_solc_files(config: &Config) -> Result<()> {
    let contracts = config.contracts();

    // The 'pre-compile' step produces an individual build folder per target chain.
    // Here we compile all contracts found in each of those build folders.
    for target in contracts.targets.keys().copied() {
        if target == Target::Stellar {
            // Stellar contracts have already been compiled
            continue;
        }

        let target_project = TargetProjectInfo::new(config, target)?;

        // Only Solc supported at the moment
        if target_project.compiler != Compiler::Solc {
            continue;
        }

        // Remove stale build artifacts
        target_project.clean()?;

        // Create new solc project for the given target dir and compile
        println!(
            "{} Solidity contracts for target {}",
            style("Compiling").bold().green(),
            style(target).bold().blue(),
        );
        let mut file_no = 0;
        let sources = target_project.contract_files().collect::<Vec<PathBuf>>();
        for file in &sources {
            file_no += 1;
            if !file.is_file() {
                bail!(
                    "Contract source file '{}' not found.  Did you run the 'pre-compile' step?",
                    file.display()
                );
            }

            let prefix = format!("[{}/{}]", file_no, sources.len());
            println!(
                "{} {} {}",
                style(prefix).bold().dim(),
                style("Compiling").bold().green().dim(),
                file.display()
            );
            // Actually compile
            let res = target_project.compile_file(file);
            match res {
                Err(err) => bail!("{}", err),
                Ok(result) => println!("  {}", style(&result.diagnostics).dim()),
            }
        }
    }
    Ok(())
}
