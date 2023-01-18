//! Support for creating new projects from built-in templates.
use color_eyre::owo_colors::OwoColorize;
use console::style;
use cubist_config::util::OrBug;
use cubist_config::{Config, ProjType};
use eyre::{bail, Result, WrapErr};
use fs_extra::dir::{move_dir, CopyOptions};
use parse_display::{Display, FromStr};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use tempfile::tempdir;

use super::CubeFactory;

static TEMPLATE_URL: &str = "git@github.com:cubist-alpha/cubist-sdk-templates";

/// Built-in project templates
#[derive(Clone, Copy, PartialEq, Eq, FromStr, Debug, Display, clap::ValueEnum)]
#[value(rename_all = "verbatim")]
pub enum Template {
    /// Counter running on Polygon and posting on Ethereum
    Storage,
    /// Multiple producer multiple consumer app running across Avalanche, Ethereum, Polygon, and an Avalanche subnet
    MPMC,
    /// Token bridge that bridges wrapped gas tokens from Avalanche to Ethereum
    TokenBridge,
}

/// Cube for git repositories
#[derive(Debug, Clone)]
pub struct TemplateCube {
    name: String,
    type_: ProjType,
    dir: PathBuf,
    template: Template,
    branch: String,
}

impl TemplateCube {
    /// Create new cube from template
    pub fn new(
        name: String,
        type_: ProjType,
        dir: PathBuf,
        template: Template,
        opt_branch: Option<String>,
    ) -> Self {
        let branch = opt_branch.unwrap_or_else(|| "main".to_string());
        TemplateCube {
            name,
            type_,
            dir,
            template,
            branch,
        }
    }
}

/// Wrapper for executing git commands
fn git_exec(args: Vec<&str>, curdir: Option<&Path>) -> Result<()> {
    let mut cmd = Command::new("git");
    cmd.args(&args)
        .stderr(Stdio::null())
        .stdout(Stdio::null())
        .stdin(Stdio::null());
    if let Some(dir) = curdir {
        cmd.current_dir(dir);
    }
    let status = cmd
        .status()
        .wrap_err(format!("Failed to execute 'git {:#?}'", &args))?;
    if !status.success() {
        bail!("'git {:#?}' exited with {}", args, status);
    }
    Ok(())
}

impl TemplateCube {
    pub fn new_project(&self, force: bool) -> Result<()> {
        if self.dir.exists() {
            bail!("Will not create template. {} exists", self.dir.display())
        }
        fs::create_dir_all(&self.dir).wrap_err("Failed to create project directory")?;
        // create temporary directory where we clone the repo
        let tmp = tempdir()?;
        let tmp_path = tmp.path();
        let tmp_str = tmp_path.to_str().or_bug("bad temp path");
        println!(
            "  {} template repository from {}",
            style("Downloading").blue().dimmed(),
            TEMPLATE_URL
        );
        // clone template repo
        git_exec(
            vec![
                "clone",
                "--depth",
                "1",
                "--branch",
                &self.branch,
                "--sparse",
                TEMPLATE_URL,
                tmp_str,
            ],
            None,
        )?;
        // checkout the template directory in the temp dir
        let template_dir = Path::new(&self.template.to_string()).join(self.type_.to_string());
        println!(
            "  {} template directory {}",
            style("Checking out").blue().dimmed(),
            template_dir.display()
        );
        git_exec(
            vec![
                "sparse-checkout",
                "set",
                &format!("{}", template_dir.display()),
            ],
            Some(tmp_path),
        )?;

        // move the template directory
        println!(
            "  {} template to {}",
            style("Copying").blue().dimmed(),
            self.dir.display()
        );
        let mut options = CopyOptions::new();
        options.content_only = true;
        move_dir(tmp_path.join(&template_dir), &self.dir, &options).wrap_err(format!(
            "move {} {} failed",
            tmp_path.join(&template_dir).display(),
            self.dir.display()
        ))?;
        // sanity check the cloned thing unless `force` is true
        if !force {
            Config::from_dir(&self.dir)?;
        }
        // update config files
        let cube = CubeFactory::create(self.type_, &self.dir)?;
        cube.update_config(&self.name)?;
        Ok(())
    }
}
