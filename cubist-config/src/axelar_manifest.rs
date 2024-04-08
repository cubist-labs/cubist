use std::fmt::{Debug, Display};

use ethers_core::types::Address;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Axelar chain names, taken from
///  - `<https://docs.axelar.dev/dev/build/chain-names/mainnet>`
///  - `<https://docs.axelar.dev/dev/build/chain-names/testnet>`
#[derive(Serialize, Deserialize, Clone, JsonSchema)]
#[allow(non_camel_case_types)]
#[allow(missing_docs)]
pub enum ChainName {
    acre,
    agoric,
    arbitrum,
    assetmantle,
    aura,
    aurora,
    Avalanche,
    Axelarnet,
    binance,
    burnt,
    celo,
    comdex,
    #[serde(rename = "comdex-2")]
    comdex_2,
    cosmoshub,
    crescent,
    #[serde(rename = "e-money")]
    e_money,
    Ethereum,
    #[serde(rename = "ethereum-2")]
    ethereum_2,
    evmos,
    Fantom,
    fetch,
    injective,
    juno,
    kava,
    ki,
    kujira,
    Moonbeam,
    optimism,
    osmosis,
    #[serde(rename = "osmosis-5")]
    osmosis_5,
    persistence,
    Polygon,
    regen,
    secret,
    sei,
    stargaze,
    terra,
    #[serde(rename = "tera-2")]
    terra_2,
    #[serde(rename = "tera-3")]
    terra_3,
    umee,
    xpla,
}

fn to_str(cn: &ChainName) -> String {
    serde_json::to_string(cn)
        .unwrap()
        .trim_matches('"')
        .to_owned()
}

impl Display for ChainName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", to_str(self))
    }
}

impl Debug for ChainName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", to_str(self))
    }
}

/// Per-target manifest file that the Axelar relayer produces
#[derive(Serialize, Deserialize, Clone, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive] // because generated json file may have other fields, e.g., 'tokens'
pub struct AxelarManifest {
    /// Chain name
    pub name: String,
    /// Chain id
    pub chain_id: u32,
    /// Gateway contract address
    #[schemars(with = "String")]
    pub gateway: Address,
    /// Gas receiver contract address
    #[schemars(with = "String")]
    pub gas_receiver: Address,
    /// Deployer contract address
    #[schemars(with = "String")]
    pub const_address_deployer: Address,
}
