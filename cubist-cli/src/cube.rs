use cubist_config::{Config, ProjType};
use cubist_sdk::core::CubistInfo;
use dialoguer::{theme::ColorfulTheme, Confirm};
use eyre::{Result, WrapErr};
use std::default::Default;
use std::fs;
use std::path::Path;

use self::{js::JavaScript, rs::Rust, ts::TypeScript};

pub mod git;
pub mod js;
pub mod rs;
pub mod template;
pub mod ts;
pub mod utils;

/// Trait implemented by all project backends
pub trait Cube {
    /// Create project
    fn new_project(&self, name: &str, force: bool) -> Result<()>;
    /// Update config (e.g., set project name, insert required dependencies, etc.)
    fn update_config(&self, name: &str) -> Result<()>;
    /// Generate ORM-style types for contracts
    fn gen_orm(&self, cubist: &CubistInfo) -> Result<()>;
}

/// Factory methods for creating concrete instances of [`Cube`].
pub struct CubeFactory;

impl CubeFactory {
    /// Create a new [`Cube`] for a given project type.
    ///
    /// # Arguments
    /// - `proj_ty`  : project type
    /// - `proj_dir` : project root directory
    pub fn create(proj_ty: ProjType, proj_dir: &Path) -> Result<Box<dyn Cube>> {
        match proj_ty {
            ProjType::JavaScript => Ok(Box::new(JavaScript::new(proj_dir.to_path_buf()))),
            ProjType::Rust => Ok(Box::new(Rust::new(proj_dir.to_path_buf()))),
            ProjType::TypeScript => Ok(Box::new(TypeScript::new(proj_dir.to_path_buf()))),
        }
    }
}

/// Create new base cube (i.e., creates project directory and config).
///
/// # Arguments
///
/// * `proj_ty`  - Project type
/// * `proj_dir` - Project directory
/// * `force`    - Force overwrite existing configuration if they exists
pub fn new_base_cube(proj_ty: ProjType, proj_dir: &Path, force: bool) -> Result<()> {
    // create project directory

    fs::create_dir_all(proj_dir).context("Failed to create project directory")?;
    // create config file
    let cfg: Config = Config::new(proj_ty, proj_dir)?;

    // If config file already exists and no overwrite, check with user
    let mut proceed = false;
    if cfg.config_path.is_file() && !force {
        proceed = Confirm::with_theme(&ColorfulTheme::default())
            .with_prompt(format!(
                "Config {} exists. Overwrite?",
                cfg.config_path.display()
            ))
            .interact()?;
        if !proceed {
            return Ok(());
        }
    }
    cfg.to_file(force || proceed).with_context(|| {
        format!(
            "Failed to save config to file {}",
            cfg.config_path.display()
        )
    })?;
    // Create contracts directory if it doesn't exist
    fs::create_dir_all(&cfg.contracts().root_dir).context("Failed to create context directory")
}
