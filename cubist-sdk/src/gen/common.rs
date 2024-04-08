//! Common types for modules in this crate
use thiserror::Error;

/// Errors that can occur in the interface generator
#[non_exhaustive]
#[derive(Error, Debug)]
pub enum InterfaceGenError {
    /// Found two contracts with the same name
    #[error("Cannot have two contracts with the same name {0}")]
    DuplicateContracts(String),
    /// No contracts found
    #[error("No contracts to transpile")]
    MissingContracts,
    /// A given path is not a file
    #[error("{0} is not a file")]
    NotAFile(String),
    /// Failed to find the function
    #[error("Could not create interface for function {0} not in source")]
    MissingFunction(String),
    /// Encountered an `SPDX-License-Identifier` directive without a license identifier
    #[error("Expected license identifier")]
    MissingLicense,
    /// Failed to generate an interface for a contract
    #[error("Cannot generate interface for {0}")]
    GenerateInterfaceError(String),
    /// Failed to find targets for a given interface
    #[error("Could not find targets for interface {0}")]
    UnknownInterface(String),
    /// Expected a certain contract that wasn't found
    #[error("Did not find expected contract {0} in sources")]
    MissingContract(String),
}

/// The standard [`Result`] type in the interface generator
pub type Result<T, E = InterfaceGenError> = core::result::Result<T, E>;
