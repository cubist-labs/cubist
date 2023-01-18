//! Support for creating TypeScript projects.
use color_eyre::owo_colors::OwoColorize;
use console::style;
use cubist_sdk::core::CubistInfo;
use cubist_util::tera::TeraEmbed;
use eyre::{Result, WrapErr};
use lazy_static::lazy_static;
use serde::Serialize;
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use tera::{Context, Tera};
use tracing::trace;
use whoami;

use super::CubeTemplates;
use crate::cube::utils::write_string_or_prompt;
use crate::cube::Cube;

lazy_static! {
    /// The codegen templates
    pub static ref ORM_TEMPLATES: Tera = CubeTemplates::tera_from_prefix("orm/ts/");
}

#[derive(Debug, Clone)]
pub struct TypeScript {
    dir: PathBuf,
}

impl TypeScript {
    /// Create TypeScript cube.
    pub fn new(dir: PathBuf) -> Self {
        TypeScript { dir }
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

impl Cube for TypeScript {
    fn new_project(&self, name: &str, force: bool) -> Result<()> {
        // Write the package.json file
        write_string_or_prompt(
            self.dir.join("package.json"),
            &serde_json::to_string_pretty(&package_json(name)?)?,
            force,
        )?;
        write_string_or_prompt(
            self.dir.join("tsconfig.json"),
            &ORM_TEMPLATES.render("tsconfig.json.tpl", &Context::new())?,
            force,
        )?;

        // create 'src/index.ts'
        let src_dir = self.src_dir();
        fs::create_dir_all(&src_dir)?;
        write_string_or_prompt(
            src_dir.join("index.ts"),
            ORM_TEMPLATES
                .render("hello.ts.tpl", &Context::new())?
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
        let paths = cubist.config().paths();
        let build_dir = cubist.config().build_dir();
        let orm_dir = build_dir.join("orm");

        // create orm dir in the build directory
        fs::create_dir_all(&orm_dir).wrap_err("Failed to create orm dir")?;

        let mut ty_export = vec![];

        for (target, contracts) in &cubist.contracts {
            let abs_target_build_dir = &paths.for_target(*target).build_root;
            let target_build_dir = match abs_target_build_dir.strip_prefix(&build_dir) {
                Ok(rel) => Path::new("..").join(rel),
                _ => abs_target_build_dir.to_path_buf(),
            };
            for contract in contracts {
                let name = &contract.fqn.name;
                let from = target_build_dir.join("types");
                ty_export.push(TyExport {
                    name: name.clone(),
                    from: from.clone(),
                });
            }
        }
        let mut tera_ctx = Context::new();
        tera_ctx.insert("ty_export", &ty_export);
        let rendered = ORM_TEMPLATES.render("index.ts.tpl", &tera_ctx)?;
        let path = &orm_dir.join("index.ts");
        fs::write(path, rendered).wrap_err("Failed to write index.ts")?;
        println!(
            "- {} {}",
            style("generated").green().dimmed(),
            path.display()
        );

        // if cubist sdk was installed (in node_modules), then generate the typechain bindings
        let mut cmd = std::process::Command::new("node");
        cmd.arg("-e");
        cmd.arg(format!(
            "require('@cubist-alpha/cubist').internal.genTypes('{}');",
            cubist.config().config_path.display()
        ));
        trace!("Running command: {:?}", cmd);
        let output = cmd
            .output()
            .wrap_err("Failed to run node to generate types")?;
        if !output.status.success() {
            trace!("Command output: {:?}", output);
            return Err(eyre::eyre!(
                "Failed to generate types. Did you forget to run 'yarn' (or 'npm i')? {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }
        println!(
            "- {} typechain bindings for all contracts",
            style("generated").green().dimmed()
        );

        Ok(())
    }
}

/// Internal type used for generating the index.ts file in the orm directory.
#[derive(Serialize)]
struct TyExport {
    /// contract name
    pub name: String,
    /// target chain type definition file
    pub from: PathBuf,
}
