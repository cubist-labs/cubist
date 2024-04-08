use clap::Parser;
use color_eyre::eyre::Result;
use cubist_config::axelar_manifest::AxelarManifest;
use cubist_config::{Config, PreCompileManifest};
use schemars::{schema::RootSchema, schema_for};
use std::fs;
use std::path::PathBuf;

/// Script for generating JSON schema for cubist-config
#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
    /// Output directory
    #[clap(value_parser)]
    out: PathBuf,
}

/// Write schema to file
fn write_schema(path: PathBuf, schema: RootSchema) -> Result<()> {
    println!("Generating {}", path.display());
    fs::write(path, serde_json::to_string_pretty(&schema)?)?;
    Ok(())
}

pub fn run(out: PathBuf) -> Result<()> {
    write_schema(out.join("config.schema.json"), schema_for!(Config))?;
    write_schema(
        out.join("pre_compile_manifest.schema.json"),
        schema_for!(PreCompileManifest),
    )?;
    write_schema(
        out.join("axelar_manifest.schema.json"),
        schema_for!(AxelarManifest),
    )?;
    Ok(())
}
