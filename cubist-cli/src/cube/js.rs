//! Support for creating JavaScript projects.
use color_eyre::owo_colors::OwoColorize;
use console::style;
use cubist_sdk::core::CubistInfo;
use cubist_util::tera::TeraEmbed;
use eyre::{Result, WrapErr};
use lazy_static::lazy_static;
use serde_json::Value;
use std::fs;
use std::path::PathBuf;
use tera::{Context, Tera};
use whoami;

use super::CubeTemplates;
use crate::cube::utils::write_string_or_prompt;
use crate::cube::Cube;

lazy_static! {
    /// The codegen templates
    pub static ref ORM_TEMPLATES: Tera = CubeTemplates::tera_from_prefix("orm/js/");
}

#[derive(Debug, Clone)]
pub struct JavaScript {
    dir: PathBuf,
}

impl JavaScript {
    /// Create JavaScript cube.
    pub fn new(dir: PathBuf) -> Self {
        JavaScript { dir }
    }

    /// Source directory
    fn src_dir(&self) -> PathBuf {
        self.dir.join("src")
    }
}

/// Create JSON object corresponding to the project package.json.
/// We currently choose defaults (e.g., for license and name), but in the future may ask for user
/// input.
fn package_json(name: &str) -> Result<Value> {
    let author = format!(
        "{} <{}@{}>",
        whoami::realname(),
        whoami::username(),
        whoami::hostname()
    );
    let mut tera_ctx = Context::new();
    tera_ctx.insert("name", &name);
    tera_ctx.insert("author", &author);
    let rendered = ORM_TEMPLATES.render("package.json.tpl", &tera_ctx)?;
    Ok(serde_json::from_str(&rendered)?)
}

impl Cube for JavaScript {
    fn new_project(&self, name: &str, force: bool) -> Result<()> {
        // Write the package.json file
        write_string_or_prompt(
            self.dir.join("package.json"),
            &serde_json::to_string_pretty(&package_json(name)?)?,
            force,
        )?;

        // create 'src/index.js'
        let src_dir = self.src_dir();
        fs::create_dir_all(&src_dir)?;
        write_string_or_prompt(
            src_dir.join("index.js"),
            ORM_TEMPLATES
                .render("hello.js.tpl", &Context::new())?
                .as_str(),
            force,
        )?;

        Ok(())
    }

    fn update_config(&self, name: &str) -> Result<()> {
        // read package.json
        let package_file = self.dir.join("package.json");
        let contents = fs::read_to_string(&package_file).wrap_err("Could not read package.json")?;
        let mut pkg: serde_json::Value = serde_json::from_str(&contents).unwrap();
        // update the name
        pkg["name"] = serde_json::json!(name);
        // write package.json
        fs::write(&package_file, serde_json::to_string_pretty(&pkg)?)
            .wrap_err("Failed to write package.json")?;
        Ok(())
    }

    fn gen_orm(&self, cubist: &CubistInfo) -> Result<()> {
        let build_dir = cubist.config().build_dir();
        let orm_dir = build_dir.join("orm");

        // create orm dir in the build directory
        fs::create_dir_all(&orm_dir).wrap_err("Failed to create orm dir")?;

        let contracts = cubist
            .contracts()
            .into_iter()
            .map(|c| c.fqn.name.clone())
            .collect::<Vec<String>>();

        let mut tera_ctx = Context::new();
        tera_ctx.insert("contracts", &contracts);
        let rendered = ORM_TEMPLATES.render("index.js.tpl", &tera_ctx)?;
        let path = &orm_dir.join("index.js");
        fs::write(path, rendered).wrap_err("Failed to write index.js")?;
        println!(
            "- {} {}",
            style("generated").green().dimmed(),
            path.display()
        );

        Ok(())
    }
}
