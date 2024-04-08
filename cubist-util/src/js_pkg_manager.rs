//! Module for interacting with JavaScript package managers. This module provides utiilities to
//! check if common package managers for JavaScript are available and provides an interface for
//! installing JavaScript packages.
use itertools::Itertools;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet as Set};
use std::io;
use std::path::Path;
use std::process::Command;
use std::str;
use thiserror::Error;
use tracing::debug;

/// Error type related to interactions with JavaScript package managers
#[derive(Debug, Error)]
pub enum Error {
    /// Error raised when we can't find a JavaScript package manager
    #[error("Could not run JavaScript package manager {0}")]
    MissingPkgManager(String),
    /// Error raised while trying to install packages
    #[error("Error installing packages: {0}")]
    InstallError(#[from] io::Error),
    /// Error raised if no package could be found for an import path
    #[error("Could not find package for: {0}")]
    PackageNotFound(String),
}

/// Result type for interactions with JavaScript package managers
pub type Result<T, E = Error> = core::result::Result<T, E>;

/// A trait that the different JavaScript package manager interfaces implement
pub trait JsPkgManager {
    /// Returns the name of the package manager
    fn name(&self) -> &str;

    /// Install a list of packages using the package manager
    ///
    /// # Arguments
    ///
    /// * `path` - The directory from which the installation is executed (this is typically the
    ///            root of the Cubist project).
    /// * `pks` - The set of packages to install
    fn install(&self, path: &Path, pkgs: &Set<String>) -> Result<()>;

    /// Checks if the package manager is available on the system
    fn is_available(&self) -> bool;

    /// Given an import path, it tries to find a matching package in the registry of the package
    /// manager. Note that finding the package is not always trivial, because some package names
    /// contain `/`.
    ///
    /// # Arguments
    ///
    /// * `imp_path` - The import path
    fn extract_pkg_from_import(&self, imp_path: &str) -> Result<String>;
}

/// An interface for the npm package manager
pub struct Npm;

impl JsPkgManager for Npm {
    fn name(&self) -> &str {
        "npm"
    }

    fn install(&self, path: &Path, pkgs: &Set<String>) -> Result<()> {
        Command::new("npm")
            .arg("install")
            .args(pkgs)
            .current_dir(path)
            .output()?;
        Ok(())
    }

    fn is_available(&self) -> bool {
        Command::new("npm")
            .arg("--version")
            .output()
            .map_or_else(|_| false, |_| true)
    }

    fn extract_pkg_from_import(&self, imp_path: &str) -> Result<String> {
        // Search for packages with names that contain the prefix of the import path
        let search_term = imp_path
            .splitn(2, '/')
            .collect::<Vec<&str>>()
            .first()
            .expect("There is always at least one element")
            .to_string();
        debug!("Searching for '{search_term}' npm packages");
        let output = Command::new("npm")
            .arg("search")
            .arg("--no-description")
            .arg("--parseable")
            .arg(search_term)
            .output()?
            .stdout;
        let pkg_list = str::from_utf8(&output).expect("Invalid npm output");

        // Find the longest package name that is a prefix of the import path
        for pkg_row in pkg_list.split('\n') {
            let pkg_name = pkg_row
                .splitn(2, '\t')
                .collect::<Vec<&str>>()
                .first()
                .expect("There is always at least one element")
                .to_string();
            let pkg_prefix = format!("{pkg_name}/");
            if imp_path.starts_with(&pkg_prefix) {
                return Ok(pkg_name);
            }
        }
        Err(Error::PackageNotFound(imp_path.to_string()))
    }
}

/// An interface for the Yarn package manager
pub struct Yarn {
    /// Memoized results
    mem: RefCell<HashMap<String, bool>>,
}

impl Yarn {
    fn new() -> Self {
        Self {
            mem: RefCell::new(HashMap::new()),
        }
    }
}

impl JsPkgManager for Yarn {
    fn name(&self) -> &str {
        "Yarn"
    }

    fn install(&self, path: &Path, pkgs: &Set<String>) -> Result<()> {
        debug!(
            "Installing {} to {}",
            pkgs.iter().join(", "),
            path.display()
        );
        Command::new("yarn")
            .arg("add")
            .args(pkgs)
            .current_dir(path)
            .output()?;
        Ok(())
    }

    fn is_available(&self) -> bool {
        Command::new("yarn")
            .arg("--version")
            .output()
            .map_or_else(|_| false, |_| true)
    }

    fn extract_pkg_from_import(&self, imp_path: &str) -> Result<String> {
        // Try different prefixes of the import path until we find one that is a valid package
        // name. Note that Yarn (1.x) does not implement a package search function.
        let mut curr_path: String = "".into();
        for part in imp_path.split('/') {
            // Update the current prefix
            if curr_path.is_empty() {
                curr_path = part.to_string();
            } else {
                curr_path = format!("{curr_path}/{part}");
            }

            // Check if we've already examined this package name
            let mut mem = self.mem.borrow_mut();
            if let Some(found) = mem.get(&curr_path) {
                if *found {
                    return Ok(curr_path);
                } else {
                    continue;
                }
            };

            // Use `yarn info <path> name` to check whether the package exists. This command
            // returns the package name if it does.
            debug!("Searching for '{curr_path}' package");
            let output = Command::new("yarn")
                .arg("info")
                .arg(&curr_path)
                .arg("name")
                .output()?
                .stdout;
            let pkg_info = str::from_utf8(&output).expect("Invalid Yarn output");
            let found = pkg_info.split('\n').any(|line| line == curr_path);
            mem.insert(curr_path.clone(), found);
            if found {
                return Ok(curr_path);
            }
        }
        Err(Error::PackageNotFound(imp_path.to_string()))
    }
}

/// A heuristic that returns the best package manager for a given path
pub fn js_pkg_manager_for_path<P: AsRef<Path>>(path: P) -> Result<Box<dyn JsPkgManager>> {
    let yarn = Yarn::new();
    if path.as_ref().join("yarn.lock").exists() {
        // If there is a `yarn.lock` file, we expect yarn to be available
        if !yarn.is_available() {
            return Err(Error::MissingPkgManager(yarn.name().to_string()));
        }
        return Ok(Box::new(yarn));
    } else if path.as_ref().join("package-lock.json").exists() {
        // If there is a `package-lock.json` file, we expect npm to be available
        if !Npm.is_available() {
            return Err(Error::MissingPkgManager(Npm.name().to_string()));
        }
        return Ok(Box::new(Npm));
    }

    // default to yarn if it exists
    if yarn.is_available() {
        Ok(Box::new(yarn))
    } else if Npm.is_available() {
        Ok(Box::new(Npm))
    } else {
        // If Yarn is not installed, we require npm to be available
        Err(Error::MissingPkgManager(format!(
            "{} OR {}",
            yarn.name(),
            Npm.name()
        )))
    }
}
