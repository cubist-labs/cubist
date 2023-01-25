use std::{collections::BTreeMap, path::PathBuf};

use serde::{Deserialize, Serialize};

use crate::{EventName, FunctionName, Target};

/// Metadata about bridging events
#[derive(Debug, Serialize, Deserialize)]
pub struct Bridge {
    /// The generated contract interfaces file. This is a path relative to the root directory.
    file: PathBuf,
    /// Source chain (i.e., the chain where the contract interface is deployed)
    sender: Target,
    /// Destination chain (i.e., the chain where the original contract is deployed)
    receiver: Target,
    /// Shim contracts defined in this interface file
    contracts: Vec<ContractBridge>,
}

/// Metadata about bridging events from a particular contract
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ContractBridge {
    /// The name of the contract
    name: String,
    /// Mapping from a function name to the name of the event it raises
    functions: BTreeMap<FunctionName, EventName>,
}

impl Bridge {
    /// Initializes a struct that holds the information about a bridge
    pub fn new(
        file: PathBuf,
        sender: Target,
        receiver: Target,
        contracts: Vec<ContractBridge>,
    ) -> Bridge {
        Bridge {
            file,
            sender,
            receiver,
            contracts,
        }
    }

    /// The chain on the receiving end of this bridge.
    pub fn receiver_target(&self) -> Target {
        self.receiver
    }

    /// Map from function name to the name of the event it raises.
    pub fn bridges(
        &self,
        contract_name: &str,
    ) -> impl Iterator<Item = (&FunctionName, &EventName)> {
        self.contracts
            .iter()
            .find(|c| c.name == contract_name)
            .map(|c| c.functions.iter())
            .unwrap()
    }
}

impl ContractBridge {
    /// Initializes a struct that holds the information about a contract bridge
    pub fn new(name: String, functions: BTreeMap<FunctionName, EventName>) -> ContractBridge {
        ContractBridge { name, functions }
    }
}
