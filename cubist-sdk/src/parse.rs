/// Module for the representation of source files and their AST
pub mod source_file;

use crate::Result;
use cubist_config::ContractsConfig;
use solang_parser::pt;
use source_file::SourceFile;

/// Parses a set of files
pub fn parse_files(config: &ContractsConfig) -> Result<Vec<SourceFile>> {
    let sources = config
        .targets
        .iter()
        .flat_map(|(target, target_config)| {
            target_config
                .contract_files()
                .iter()
                .map(|file| SourceFile::new(file, config.relative_to_root(file).unwrap(), *target))
                .collect::<Vec<_>>()
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(sources)
}

/// Returns the path part of an import (i.e., the part that refers to the actual file to import).
pub fn get_import_path(import: &pt::Import) -> &String {
    match import {
        pt::Import::Plain(s, ..) => &s.string,
        pt::Import::GlobalSymbol(s, ..) => &s.string,
        pt::Import::Rename(s, ..) => &s.string,
    }
}
