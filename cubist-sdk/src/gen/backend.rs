//! The different back ends for the interface generator. Each supported relay provider has an
//! associated backend. The back ends process interface information and generate interface and
//! configuration files.
use crate::gen::common::Result;
use crate::gen::interface::file::FileInterfaces;
use cubist_config::bridge::{Bridge, ContractBridge};
use cubist_config::util::OrBug;
use cubist_config::{ContractName, Target};
use cubist_util::tera::TeraEmbed;
use lazy_static::lazy_static;
use rust_embed::RustEmbed;
use std::collections::BTreeMap;
use std::path::PathBuf;
use tera::{Context, Tera};

use super::APPROVE_CALLER_METHOD_NAME;

#[derive(RustEmbed)]
#[folder = "$CARGO_MANIFEST_DIR/templates"]
struct CubeTemplates;
impl TeraEmbed for CubeTemplates {}

lazy_static! {
    /// The codegen templates
    pub static ref TEMPLATES: Tera = CubeTemplates::tera_from_prefix("");
}

/// Metadata associated with an artifact generated by a back end. The metadata that we store
/// depends on the type of artifact.
pub enum ArtifactMetadata {
    /// No additional metadata
    Empty,
    /// The contracts that this artifact contains shims for
    ContractShims {
        /// The name of the artifact
        source_file: PathBuf,
        /// The contract shims in the artifact
        contracts: Vec<ContractName>,
    },
}

/// An artifact generated by a back end (e.g., an interface contract or a configuration file)
pub struct Artifact {
    /// The chain that this artifact is associated with
    target: Target,
    /// The name of this artifact
    name: PathBuf,
    /// The content of this artifact
    content: String,
    /// The type-dependent metadata associated with the generated artifact
    metadata: ArtifactMetadata,
}

impl Artifact {
    /// Returns the target chain for this artifact
    pub fn target(&self) -> Target {
        self.target
    }

    /// Returns the name of this artifact
    pub fn name(&self) -> &PathBuf {
        &self.name
    }

    /// Returns the content of this artifact
    pub fn content(&self) -> &String {
        &self.content
    }

    /// Returns the metadata of this artifact
    pub fn metadata(&self) -> &ArtifactMetadata {
        &self.metadata
    }
}

/// A back end that processes interface information and returns a list of artifacts
pub trait Backend {
    /// The name of the back end
    fn name(&self) -> &'static str;
    /// Processes a single interface file
    fn process(&self, file: &FileInterfaces) -> Result<Vec<Artifact>>;
}

/// The back end for the Cubist relayer
pub struct CubistBackend;

impl Backend for CubistBackend {
    fn name(&self) -> &'static str {
        "cubist"
    }

    fn process(&self, file: &FileInterfaces) -> Result<Vec<Artifact>> {
        let file_name = file.get_source_file();
        let mut result = vec![];

        // Generate the bridge configuration file
        let contracts: Vec<ContractBridge> = file
            .interfaces
            .iter()
            .map(|contract| {
                let functions = contract
                    .get_functions()
                    .iter()
                    .map(|function| {
                        (
                            function.name().clone(),
                            format!(
                                "__cubist_event_{}_{}",
                                contract.get_contract_name(),
                                function.name()
                            ),
                        )
                    })
                    .collect::<BTreeMap<_, _>>();
                ContractBridge::new(contract.get_contract_name().clone(), functions)
            })
            .collect();
        let bridge = Bridge::new(
            file.get_source_file().clone(),
            file.get_sender_target(),
            file.get_receiver_target(),
            contracts,
        );
        result.push(Artifact {
            target: file.get_sender_target(),
            name: file_name.with_extension("bridge.json"),
            content: serde_json::to_string_pretty(&bridge).or_bug("Serializing bridge file"),
            metadata: ArtifactMetadata::Empty,
        });

        // Generate the interface file
        let mut context = Context::new();
        let contract_shims: Vec<String> = file
            .interfaces
            .iter()
            .map(|contract| contract.get_contract_name().clone())
            .collect();
        context.insert("file", file);
        context.insert("APPROVE_CALLER_METHOD_NAME", APPROVE_CALLER_METHOD_NAME);
        result.push(Artifact {
            target: file.get_sender_target(),
            name: file_name.clone(),
            content: TEMPLATES
                .render("cubist_sender.tpl", &context)
                .or_bug("Rendering 'cubist_sender' template"),
            metadata: ArtifactMetadata::ContractShims {
                source_file: file.get_source_file().clone(),
                contracts: contract_shims,
            },
        });

        Ok(result)
    }
}

/// The back end for the Axelar relayer
pub struct AxelarBackend;

impl Backend for AxelarBackend {
    fn name(&self) -> &'static str {
        "axelar"
    }

    fn process(&self, file: &FileInterfaces) -> Result<Vec<Artifact>> {
        let file_name = file.get_source_file();
        let mut result = vec![];

        let mut context = Context::new();
        context.insert("file", file);

        // Generate the receiver file
        result.push(Artifact {
            target: file.get_receiver_target(),
            name: file_name.with_extension("receiver.sol"),
            content: TEMPLATES
                .render("axelar_receiver.tpl", &context)
                .or_bug("Rendering 'axelar_receiver' template"),
            metadata: ArtifactMetadata::Empty,
        });
        // Generate the sender file
        let contract_shims: Vec<String> = file
            .interfaces
            .iter()
            .map(|contract| contract.get_contract_name().clone())
            .collect();
        result.push(Artifact {
            target: file.get_sender_target(),
            name: file_name.clone(),
            content: TEMPLATES
                .render("axelar_sender.tpl", &context)
                .or_bug("Rendering 'axelar_sender' template"),
            metadata: ArtifactMetadata::ContractShims {
                source_file: file.get_source_file().clone(),
                contracts: contract_shims,
            },
        });
        Ok(result)
    }
}
