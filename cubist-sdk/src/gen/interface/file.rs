//! Generate cross-chain interfaces for a whole file of contracts
use crate::gen::common::{InterfaceGenError, Result};
use crate::gen::interface::config::InterfaceConfig;
use crate::gen::interface::contract::ContractInterface;
use crate::gen::interface::import::Import;
use crate::parse::source_file::SourceFile;
use cubist_config::Target;
use serde::{Serialize, Serializer};
use solang_parser::pt;
use solang_parser::pt::Docable;
use std::fmt;
use std::path::PathBuf;

/// All the contract interfaces corresponding to a particular file.
/// If a file has four contracts and three are used cross chain,
/// FileInterfaces's interfaces will contain three contracts
#[derive(Debug, Serialize)]
pub struct FileInterfaces {
    /// Information about the source file (e.g., full path)
    source_info: SourceInfo,

    /// The sender chain's target
    /// (This is the target where we're exposing our contracts, not the target where
    /// the contracts originally lived. If we're exposing EthStorage on Avalanche,
    /// sender_target is Avalanche (because that's the target sending messaged to Eth)).
    sender_target: Target,

    /// The receiver chain's target
    /// (This is the target where the original contract we're exposing is deployed.
    /// If we're exposing EthStorage on Avalanche, this target is still Ethereum.
    receiver_target: Target,

    /// The file's pragmas
    pragmas: Vec<Pragma>,

    /// The file's imports
    imports: Vec<Import>,

    /// The file's license.
    license: Option<String>,

    /// All the interfaces that correspond to contracts in a given file.
    /// If there are two contracts in the file, there will be two generated interfaces
    pub interfaces: Vec<ContractInterface>,
}

/// Information about a source file
/// Right now, this just contains a full path and a file path (the latter as an easy
/// hack for working with our templating library). Later we'll need more information
/// like language, I'm guessing.
#[derive(Debug, Serialize)]
pub struct SourceInfo {
    #[serde(skip_serializing)]
    full_path: PathBuf,
    rel_path: PathBuf,
}

/// A pragma directive in a contract
#[derive(Debug)]
pub struct Pragma(pt::SourceUnitPart);

/// Custom serialize for pragma
/// This is necessary because our templating library hacks into serialization
impl Serialize for Pragma {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let str = self.to_string();
        serializer.serialize_str(&str)
    }
}

impl fmt::Display for Pragma {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.0.display())
    }
}

/// Necessary for Tera, as stated above
impl Serialize for Import {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let str = self.to_string();
        serializer.serialize_str(&str)
    }
}

impl FileInterfaces {
    /// Given a source file and a config, generate interfaces for every contract in that
    /// file for a given target
    pub fn new(source: &SourceFile, config: &InterfaceConfig, target: Target) -> Result<Self> {
        let license = get_license(&source.comments)?;
        let mut pragmas = Vec::new();
        let mut imports = Vec::new();
        let mut interfaces = Vec::new();
        for part in &source.pt.0 {
            if let pt::SourceUnitPart::ImportDirective(_) = part {
                imports.push(part.clone());
            } else if let pt::SourceUnitPart::ContractDefinition(cd) = part {
                let contract_name = &cd.name.name;
                // Are we supposed to generate an interface for this contract?
                if !config.gen_contract(contract_name) {
                    continue;
                }
                let interface = ContractInterface::new(config, cd)?;
                interfaces.push(interface);
            } else if let pt::SourceUnitPart::PragmaDirective(..) = part {
                pragmas.push(Pragma(part.clone()));
            }
        }

        if !source.file_name.is_file() {
            return Err(InterfaceGenError::NotAFile(
                source.file_name.display().to_string(),
            ));
        }

        Ok(FileInterfaces {
            source_info: SourceInfo {
                full_path: source.file_name.clone(),
                rel_path: source.rel_path.clone(),
            },
            sender_target: target,
            receiver_target: source.target,
            pragmas,
            imports: imports.iter().map(Import::new).collect(),
            license,
            interfaces,
        })
    }

    /// Returns true if this contract contains no interfaces
    pub fn is_empty(&self) -> bool {
        self.interfaces.is_empty()
    }

    /// Returns the chain that we're exposing the contract to
    /// (e.g., Avalanche if we're exposing EthStorage to Avalanche).
    pub fn get_sender_target(&self) -> Target {
        self.sender_target
    }

    /// Returns the chain that the original contract lives on
    pub fn get_receiver_target(&self) -> Target {
        self.receiver_target
    }

    /// Returns the full path of the source file
    pub fn get_source_path(&self) -> &PathBuf {
        &self.source_info.full_path
    }

    /// Returns *just* the source file relative to the contracts root dir.
    /// for /foo/bar/contracts/baz/Eth.sol, this will return baz/Eth.sol
    pub fn get_source_file(&self) -> &PathBuf {
        &self.source_info.rel_path
    }

    /// Returns the file stem of source file, e.g., if the source file is `eth.sol`,
    /// this method returns `eth`
    pub fn get_file_stem(&self) -> Option<String> {
        self.source_info
            .rel_path
            .file_stem()
            .and_then(|f| f.to_str().map(|s| s.to_string()))
    }
}

/// Given a vector of comments, return a string containing just the license
fn get_license(comments: &[pt::Comment]) -> Result<Option<String>> {
    if comments.is_empty() {
        // No comment means no license
        return Ok(None);
    } else {
        // The license is supposed to be the first thing in the comments
        let contents = comments[0].get_contents();
        // Solidity will error if there are multiple licenses,
        // so we don't check that case: compiling the original
        // contract at the next stage will result in an error
        if let Some((_, after)) = contents.split_once("SPDX-License-Identifier:") {
            // There has to be *some* license after the license identifier
            if after.is_empty() {
                return Err(InterfaceGenError::MissingLicense);
            }
            // Find the first thing after the license identifier
            let mut license = None;
            for word in after.split(' ').collect::<Vec<&str>>() {
                if !word.is_empty() {
                    license = Some(word.to_string());
                    break;
                }
            }
            if license.is_none() {
                return Err(InterfaceGenError::MissingLicense);
            }
            return Ok(license);
        }
    }
    Ok(None)
}
