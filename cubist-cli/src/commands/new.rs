use console::style;
use eyre::{Result, WrapErr};
use std::path::Path;

use crate::cube::git::{Git, GitUrl};
use crate::cube::template::{Template, TemplateCube};
use crate::cube::{new_base_cube, CubeFactory};
use cubist_config::ProjType;

/// Command that creates new empty project
///
/// # Arguments
///
/// * `name`  - Project name
/// * `type_` - Project type/language
/// * `dir`   - Directory to create the project in
/// * `force` - Force overwrite existing configuration if one exists
pub fn empty(name: &str, type_: ProjType, dir: &Path, force: bool) -> Result<()> {
    println!(
        "{} new {} project {} in {}",
        style("Creating").bold().green(),
        style(type_).bold(),
        style(name).bold().green(),
        dir.display()
    );
    let proj_dir = dir.join(name);
    new_base_cube(type_, &proj_dir, force).wrap_err("Failed to create base cube")?;
    let cube = CubeFactory::create(type_, &proj_dir)?;
    cube.new_project(name, force)
}

/// Command that creates new project from template.
///
/// # Arguments
///
/// * `name`     - Project name
/// * `type_`    - Project type/language
/// * `template` - Template
/// * `dir`      - Directory to create the project in
/// * `force`    - Don't sanity check the cloned template
/// * `branch`   - Branch for template repo to use (typically just for tests)
pub fn from_template(
    name: &str,
    type_: ProjType,
    template: Template,
    dir: &Path,
    force: bool,
    branch: Option<String>,
) -> Result<()> {
    println!(
        "{} new {}-{} project {} in {}",
        style("Creating").bold().green(),
        style(template).bold(),
        style(type_).bold(),
        style(name).bold().green(),
        dir.display()
    );
    let proj_dir = dir.join(name);
    let cube = TemplateCube::new(name.to_string(), type_, proj_dir, template, branch);
    cube.new_project(force)
}

/// Command that creates new project from git repo.
///
/// # Arguments
///
/// * `name`  - Project name
/// * `url`   - URL to git template repository
/// * `dir`   - Directory to create the project in
/// * `force` - Don't sanity check the cloned template
pub fn from_git_repo(name: &str, url: &GitUrl, dir: &Path, force: bool) -> Result<()> {
    println!(
        "{} new {} project from git repo {} in {}",
        style("Creating").bold().green(),
        style(name).bold().green(),
        url,
        dir.display()
    );
    let proj_dir = dir.join(name);
    let cube = Git::new(proj_dir, url);
    cube.new_project(force)
}

#[cfg(test)]
mod tests {
    use super::*;
    use cubist_config::Config;
    use serde_json::{json, Value};
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_new_js() {
        let tmp = tempdir().unwrap();
        let name = "myApp".to_string();
        empty(&name, ProjType::JavaScript, tmp.path(), false).unwrap();
        assert!(
            tmp.path().join("myApp").is_dir(),
            "Project directory exists"
        );

        // sanity check cubist config
        let config_file = tmp.path().join("myApp").join("cubist-config.json");
        assert!(config_file.is_file(), "cubist-config.json exists");
        let cfg = Config::from_file(config_file).unwrap();
        assert_eq!(cfg.type_, ProjType::JavaScript);

        // sanity check package.json
        let package_file = tmp.path().join("myApp").join("package.json");
        assert!(package_file.is_file(), "package.json exists");
        let contents = fs::read_to_string(package_file).unwrap();
        let pkg: Value = serde_json::from_str(&contents).unwrap();
        assert_eq!(pkg["name"], json!(name));
        assert!(pkg["version"].is_string());
        assert!(pkg["description"].is_string());
        assert!(pkg["license"].is_string());
        assert!(pkg["dependencies"].is_object());
        assert!(pkg["dependencies"]
            .as_object()
            .unwrap()
            .contains_key("@cubist-labs/cubist"));
    }
}
