//! Support for creating Rust projects.
use color_eyre::owo_colors::OwoColorize;
use console::style;
use cubist_config::Target;
use cubist_sdk::core::{ContractInfo, CubistInfo};
use cubist_util::tera::TeraEmbed;
use ethers_contract_abigen::Abigen;
use eyre::{Result, WrapErr};
use lazy_static::lazy_static;
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};
use tera::{Context, Tera};
use toml::{toml, Value};

use super::CubeTemplates;
use crate::cube::utils::write_string_or_prompt;
use crate::cube::Cube;

lazy_static! {
    /// The codegen templates
    pub static ref ORM_TEMPLATES: Tera = CubeTemplates::tera_from_prefix("orm/rust/");
}

#[derive(Debug, Clone)]
pub struct Rust {
    dir: PathBuf,
}

/// Create TOML [`Value`] instance corresponding to the project Cargo.toml.
/// We currently choose defaults (e.g., for version and edition), but in the future may ask for user
/// input.
pub fn cargo_toml(name: &str) -> Value {
    let cubist_git_url = "ssh://git@github.com/cubist-alpha/cubist.git";
    toml! {
        [package]
        name = name
        version = "0.1.0"
        edition = "2021"

        [dependencies]
        cubist-sdk = { git = cubist_git_url, package = "cubist-sdk" }
        cubist-config = { git = cubist_git_url, package = "cubist-config" }
        ethers = { version = "1.0.2", features = ["abigen"] }
        tokio = "1.21.2"
        lazy_static = "1.4.0"
    }
}

impl Cube for Rust {
    fn new_project(&self, name: &str, force: bool) -> Result<()> {
        // Write the Cargo.toml file
        write_string_or_prompt(
            self.dir.join("Cargo.toml"),
            &toml::to_string_pretty(&cargo_toml(name))?,
            force,
        )?;

        // create 'src/main.rs'
        let src_dir = self.src_dir();
        fs::create_dir_all(&src_dir)?;
        write_string_or_prompt(
            src_dir.join("main.rs"),
            r###"
#![allow(non_snake_case)]
#![allow(unused_imports)]

use cubist_sdk::core::*;
use cubist_sdk::contracts::*;

fn main() {
    println!("Hello from Cubist dApp");
}"###,
            force,
        )?;

        Ok(())
    }

    fn update_config(&self, name: &str) -> Result<()> {
        // read Cargo.toml
        let cargo_file = self.dir.join("Cargo.toml");
        let contents = fs::read_to_string(&cargo_file).wrap_err("Could not read Cargo.toml")?;
        let mut tml: toml::Value = toml::from_str(&contents).unwrap();
        // update the name
        let inserted = tml
            .get_mut("package")
            .expect("Expected to find 'package' value in top-level Cargo.toml")
            .as_table_mut()
            .expect("Expected top-level 'package' value to be a table")
            .insert(String::from("name"), toml::Value::String(name.to_string()))
            .is_some();
        // write Cargo.toml
        if inserted {
            fs::write(&cargo_file, toml::to_string_pretty(&tml)?)
                .wrap_err("Failed to write Cargo.toml")?;
        }
        Ok(())
    }

    fn gen_orm(&self, cubist: &CubistInfo) -> Result<()> {
        // ensure 'src/cubist' dir exists
        let contracts_dir = self.contracts_dir();
        fs::create_dir_all(&contracts_dir)?;

        let log_generated = |path: &Path| {
            println!(
                "- {} {}",
                style("generated").green().dimmed(),
                path.display(),
            )
        };

        // write modules for individual contracts
        let mut mods = Vec::new();
        for (t, cs) in &cubist.contracts {
            for c in cs {
                let abi_json = serde_json::to_string(&c.abi)?;
                let bindings = Abigen::new(&c.fqn.name, abi_json)?.generate()?;
                let file = contracts_dir.join(bindings.module_filename());
                bindings.write_to_file(&file)?;
                log_generated(&file);
                mods.push((bindings.module_name(), c, t));
            }
        }

        // write module file
        let mod_rs = contracts_dir.with_extension("rs");
        let mod_rs_contents = self.gen_cubist_rs_content(cubist, mods)?;
        fs::write(&mod_rs, mod_rs_contents)?;
        log_generated(&mod_rs);
        Ok(())
    }
}

#[derive(Serialize)]
struct TeraContract {
    pub struct_name: String,
    pub rs_mod_name: String,
    pub static_name: String,
    pub target: String,
    pub shim_targets: Vec<String>,
}

impl Rust {
    /// Create Rust cube.
    pub fn new(dir: PathBuf) -> Self {
        Rust { dir }
    }

    /// Source directory
    fn src_dir(&self) -> PathBuf {
        self.dir.join("src")
    }

    fn contracts_dir(&self) -> PathBuf {
        self.src_dir().join("cubist_gen")
    }

    fn gen_cubist_rs_content(
        &self,
        cubist: &CubistInfo,
        mods: Vec<(String, &ContractInfo, &Target)>,
    ) -> Result<String> {
        let mut tera_ctx = Context::new();
        tera_ctx.insert(
            "contracts",
            &mods
                .into_iter()
                .map(|(mod_name, c, t)| TeraContract {
                    rs_mod_name: mod_name.clone(),
                    struct_name: c.fqn.name.clone(),
                    static_name: format!("CUBIST_{}", mod_name.to_uppercase()),
                    target: format!("{:?}", t),
                    shim_targets: cubist
                        .shim_targets(&c.fqn)
                        .map(|t| format!("{:?}", t))
                        .collect::<Vec<_>>(),
                })
                .collect::<Vec<_>>(),
        );

        let rendered = ORM_TEMPLATES.render("cubist_gen_rs.tpl", &tera_ctx)?;
        Ok(rendered)
    }
}
