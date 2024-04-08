use console::style;
use cubist_sdk::gen::backend::{Artifact, ArtifactMetadata, Backend};
use cubist_sdk::gen::interface::Interfaces;
use cubist_sdk::parse::{get_import_path, parse_files, source_file::SourceFile};
use eyre::{bail, eyre, Result, WrapErr};
use itertools::Itertools;
use std::collections::HashSet as Set;
use std::process::Command;
use std::{collections::HashMap, fs, path::Path};

use cubist_config::{
    paths::{Paths, TargetPaths},
    CompilerConfig, Config, ContractsConfig, FileArtifact, PreCompileManifest, Target,
    TargetConfig,
};
use cubist_sdk::core::validate_file;
use cubist_util::js_pkg_manager::js_pkg_manager_for_path;
use tracing::{debug, warn};

pub struct PreCompiler {
    /// Well-known paths.
    paths: Paths,

    /// Parsed cubist-config.json
    contracts: ContractsConfig,

    /// The bridge provider (e.g., Cubist or Axelar)
    backend: Box<dyn Backend>,

    /// Generated interfaces, as returned by the interface generator.
    interfaces: Interfaces,
}

impl PreCompiler {
    pub fn new(config: &Config) -> Result<Self> {
        let paths = config.paths();
        fs::create_dir_all(&paths.build_dir)?;

        let contracts = config.contracts().clone();
        contracts
            .targets
            .iter()
            .try_for_each(|(target, target_config)| match target_config {
                TargetConfig::StellarTargetConfig { root, .. } => {
                    assert_eq!(*target, Target::Stellar);
                    println!(
                        "{} Soroban workspace at {}",
                        style("Compiling").bold().green(),
                        root.display()
                    );
                    Command::new("soroban")
                        // Ensure that we are not using the toolchain of the parent process
                        .env_remove("RUSTUP_TOOLCHAIN")
                        .current_dir(root)
                        .args(["contract", "build"])
                        .status()
                        .wrap_err("Failed to execute Soroban")?;
                    let missing_files: Vec<_> = target_config
                        .contract_files()
                        .into_iter()
                        .filter(|file| !file.exists())
                        .collect();
                    if !missing_files.is_empty() {
                        bail!("Missing contract files: {:?}", missing_files);
                    }
                    Ok(())
                }
                _ => Ok(()),
            })?;

        // Parse contract files
        let source_files = parse_files(&contracts)?;

        debug!("Validating imports");
        Self::validate_imports(config, &source_files)?;
        debug!("Checking external dependencies");
        Self::fetch_external_imports(config, &paths, &source_files)?;

        // We currently cannot validate source files in projects that have Stellar targets (we
        // would have to compile the Soroban contracts before validating the source files)
        if !contracts.targets.contains_key(&Target::Stellar) {
            debug!("Validating Solidity source files");
            Self::validate_solidity_source_files(&config.get_compiler_config(), &contracts)?;
        } else {
            warn!("Skipping the validation of Solidity source files due to Soroban contracts");
        }

        // get interfaces to expose contracts cross-chain
        let interfaces = Interfaces::new(&source_files)?;
        Ok(PreCompiler {
            paths,
            contracts,
            backend: <dyn Backend>::create(config),
            interfaces,
        })
    }

    /// Fetches imports from external sources (e.g., npm packages)
    fn fetch_external_imports(
        config: &Config,
        paths: &Paths,
        source_files: &[SourceFile],
    ) -> Result<()> {
        let cc = config.get_compiler_config();

        // Extract import paths from solidity source files
        debug!(
            "Extracting import paths from {} source files",
            source_files.len()
        );
        let imp_paths = source_files.iter().flat_map(|source| {
            source
                .import_directives()
                .into_iter()
                .map(|imp_path| get_import_path(&imp_path).clone())
        });
        // Filter out the paths that are actually JavaScript imports (they have format `@<import>`)
        // and do not already exist in one of the import directories
        let missing_imports = imp_paths
            .filter(|imp_path| imp_path.starts_with('@') && cc.search(imp_path).is_none())
            .collect::<Set<_>>();

        let backend = <dyn Backend>::create(config);
        let missing_deps = backend
            .npm_dependencies()
            .into_iter()
            .filter(|t| cc.search(&t.0).is_none())
            .collect::<Set<_>>();

        // Return if no external packages were found
        if missing_imports.is_empty() && missing_deps.is_empty() {
            debug!("No external imports found");
            return Ok(());
        }

        // Return error if external packages were found but we are not allowed to download them
        if !config.allow_import_from_external() {
            let import_paths = missing_imports
                .iter()
                .chain(missing_deps.iter().map(|t| &t.0))
                .map(|p| format!("\n  {p}"))
                .join("");
            let msg = format!(
                "Found external imports that are currently missing:
                {import_paths}
                \nSet `allow_import_from_external` to `true` in cubist-config.json to allow cubist to automatically download them");
            return Err(eyre!(msg));
        }

        // Determine the JavaScript package manager to use
        debug!("Determining which package manager to use");
        let js_pkg_manager = js_pkg_manager_for_path(&paths.project_dir)
            .wrap_err("Failed to determine JS package manager")?;

        // Extract packages to install based on import paths
        debug!(
            "Extracting package names from {} imports",
            missing_imports.len()
        );
        let js_pkgs = missing_imports
            .iter()
            .map(|imp_path| js_pkg_manager.extract_pkg_from_import(imp_path))
            .chain(missing_deps.iter().map(|p| Ok(format!("{}@{}", p.0, p.1))))
            .collect::<Result<Set<_>, _>>()?
            .iter()
            .map(|pkg| {
                // Use dependency versions from the config file if available
                config
                    .contracts()
                    .solidity_dependencies
                    .get(pkg)
                    .map(|version| format!("{}@{}", pkg, version))
                    .unwrap_or_else(|| pkg.to_owned())
            })
            .collect::<Set<String>>();

        // Install packages
        let pkg_names = js_pkgs.iter().join(",");
        println!(
            "{} {pkg_names} with {}",
            style("Installing").bold().green(),
            js_pkg_manager.name()
        );
        js_pkg_manager
            .install(&paths.project_dir, &js_pkgs)
            .map_err(|e| {
                eyre!(
                    "Failed to install packages {pkg_names} with {}: {}",
                    js_pkg_manager.name(),
                    e
                )
            })?;

        Ok(())
    }

    /// Checks for imports that will cause breakage when Cubist copies files
    pub fn validate_imports(config: &Config, source_files: &[SourceFile]) -> Result<()> {
        source_files.iter().try_for_each(|file| {
            file.check_imports(config).wrap_err(format!(
                "Import errors in contract {}",
                file.file_name.display()
            ))
        })
    }

    /// Attempts to compile the original source files. This ensures that the later steps in our
    /// compilation process deal with sane source files.
    pub fn validate_solidity_source_files(
        compiler_config: &CompilerConfig,
        contracts: &ContractsConfig,
    ) -> Result<()> {
        for (target, target_config) in &contracts.targets {
            match target_config {
                TargetConfig::EvmTargetConfig { compiler, .. } => {
                    let sources = target_config.source_files();
                    println!(
                        "{} {} file(s) for target {}",
                        style("Validating").bold().green(),
                        &sources.len(),
                        style(target).bold().blue()
                    );
                    sources
                        .iter()
                        .map(|src| {
                            validate_file(compiler, compiler_config, src)
                                .wrap_err(format!("Failed to validate {}", src.display()))
                        })
                        .collect::<Result<Vec<()>>>()?;
                }
                TargetConfig::StellarTargetConfig { .. } => {
                    // Soroban files have already been compiled at this point. No need to validate.
                    continue;
                }
            }
        }
        Ok(())
    }

    /// Generates build folders for all targets
    fn generate_all(&self) -> Result<()> {
        let mut artifacts: HashMap<Target, Vec<Artifact>> = HashMap::new();
        for interface in &self.interfaces.interfaces {
            let interface_artifacts = self.backend.process(interface)?;
            for interface_artifact in interface_artifacts {
                artifacts
                    .entry(interface_artifact.target())
                    .or_default()
                    .push(interface_artifact);
            }
        }

        let no_artifacts: Vec<Artifact> = vec![];
        for (target, target_config) in &self.contracts.targets {
            let target_paths = self.paths.for_target(*target);
            println!(" {}", style(target).bold().blue());
            self.prepare_target_dir(&self.contracts.root_dir, &target_paths.contracts)?;
            let target_artifacts = artifacts.get(target).unwrap_or(&no_artifacts);
            self.generate_target(target_paths, target_config, target_artifacts)?;
        }

        Ok(())
    }

    /// Prepares the target dir: makes sure that it exists and that it is empty
    fn prepare_target_dir(&self, root_dir: &Path, target_dir: &Path) -> Result<()> {
        // delete any stale files first (if any)
        if target_dir.is_dir() {
            fs::remove_dir_all(target_dir).wrap_err(format!(
                "Failed to delete build directory {}",
                target_dir.display()
            ))?;
        }

        // ensure target folder exists
        println!(
            " - copying {} -> {}",
            root_dir.display(),
            target_dir.display()
        );
        let mut opts = fs_extra::dir::CopyOptions::new();
        opts.copy_inside = true;
        fs_extra::dir::copy(root_dir, target_dir, &opts).wrap_err(format!(
            "Failed to copy '{}' to '{}'",
            root_dir.display(),
            target_dir.display(),
        ))?;

        Ok(())
    }

    /// Generates a build folder for a given target:
    /// - copies the source contracts corresponding to that target
    /// - writes out artifacts for all interfaces associated with the given target
    fn generate_target(
        &self,
        target_paths: &TargetPaths,
        target_config: &TargetConfig,
        target_artifacts: &Vec<Artifact>,
    ) -> Result<()> {
        // copy contracts corresponding to the target chain
        let mut files = Vec::new();
        for contract_file in target_config.contract_files() {
            let contract_rel_path = self.contracts.relative_to_root(&contract_file)?;
            files.push(FileArtifact::native_contract(
                contract_rel_path,
                self.interfaces.get_contracts_in_file(&contract_file),
            ));
        }

        for target_artifact in target_artifacts {
            let target_file = target_paths.contracts.join(target_artifact.name());
            debug!("About to generate {}", target_file.display());
            fs::create_dir_all(target_file.parent().unwrap())?;
            fs::write(&target_file, target_artifact.content())?;

            if let ArtifactMetadata::ContractShims { contracts } = target_artifact.metadata() {
                let artifact_name = target_artifact.name().to_owned();
                files.push(FileArtifact::shim(artifact_name, contracts.clone()));
            }
            println!(" - generated {}", target_file.display());
        }

        debug!(
            "Saving compile manifest at {}",
            target_paths.manifest.display()
        );
        let manifest = PreCompileManifest { files };
        manifest.to_file(&target_paths.manifest)?;
        println!(" - generated {}", target_paths.manifest.display());
        Ok(())
    }
}

/// Command that prepares the project for compilation.
///
/// This create a root-directory, per target chain and generates any contract interfaces for
/// contracts called cross-chain.
pub fn pre_compile(config: &Config) -> Result<()> {
    println!("{} project", style("Pre-compiling").bold().green());

    let compiler = PreCompiler::new(config)?;
    compiler.generate_all()
}
