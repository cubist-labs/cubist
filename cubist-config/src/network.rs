use std::path::PathBuf;

use coins_bip39::{English, Mnemonic};
use k256::SecretKey;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{
    secret::{Secret, SecretUrl},
    Target,
};

/// The configuration for a suite of endpoints.
/// Used to specify a single or multi-chain environment
#[derive(Debug, Default, Deserialize, Serialize, JsonSchema, Clone)]
#[serde(deny_unknown_fields)]
pub struct NetworkProfile {
    /// configuration for an ethereum endpoint
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ethereum: Option<EthereumConfig>,
    /// configuration for an avalanche endpoint
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avalanche: Option<AvalancheConfig>,
    /// configuration for a polygon endpoint
    #[serde(skip_serializing_if = "Option::is_none")]
    pub polygon: Option<PolygonConfig>,
    /// configuration for a avalanche subnet endpoint
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ava_subnet: Option<AvalancheConfig>,
}

impl NetworkProfile {
    /// Attempts to look up the config for an endpoint by target
    pub fn get(&self, name: Target) -> Option<EndpointConfig> {
        match name {
            Target::Ethereum => self.ethereum.clone().map(EndpointConfig::Eth),
            Target::Avalanche => self.avalanche.clone().map(EndpointConfig::Ava),
            Target::Polygon => self.polygon.clone().map(EndpointConfig::Poly),
            Target::AvaSubnet => self
                .ava_subnet
                .clone()
                .map(AvalancheConfig::with_default_subnet)
                .map(EndpointConfig::AvaSub),
        }
    }
}

/// Configuration for an unspecified network
#[derive(Clone, Debug)]
pub enum EndpointConfig {
    /// An ethereum config
    Eth(EthereumConfig),
    /// An Avalanche config
    Ava(AvalancheConfig),
    /// A Polygon config
    Poly(PolygonConfig),
    /// An Avalanche subnet config
    AvaSub(AvalancheConfig),
}

/// Configuration for mnemonic-based credentials
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MnemonicConfig {
    /// The bip39 english string used as the seed for generating accounts
    pub seed: Secret<Mnemonic<English>>,
    /// The number of accounts to generate using the mnemonic
    #[serde(default = "one")]
    pub account_count: u16,
    /// The derivation path, or None for the default `m/44’/60’/0’/0/`
    #[serde(default = "default_derivation_path")]
    pub derivation_path: String,
}

/// Configuration for keystore-based credentials
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct KeystoreConfig {
    /// Encrypted keystore
    pub file: PathBuf,
    /// Password for decrypting the keystore
    pub password: Secret,
}

/// Configuration for private key-based credentials
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PrivateKeyConfig {
    /// Hex-encoded private key (should not start with "0x")
    pub hex: Secret<SecretKey>,
}

/// Different ways to configure credentials
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub enum CredConfig {
    /// Mnemonic-based
    #[serde(rename = "mnemonic")]
    Mnemonic(MnemonicConfig),
    /// Keystore-based
    #[serde(rename = "keystore")]
    Keystore(KeystoreConfig),
    /// Private key-based, hex-encoded private key (should not start with "0x")
    #[serde(rename = "private_key")]
    PrivateKey(PrivateKeyConfig),
}

/// Proxy configuration
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ProxyConfig {
    /// Local port where the proxy will run
    pub port: u16,
    /// Credentials configuration    
    pub creds: Vec<CredConfig>,
    /// Chain id (transaction chain ID must be set before signing)
    pub chain_id: u32,
}

/// Contains the config options that are common to all providers
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CommonConfig {
    /// Url the endpoint can be found at
    pub url: SecretUrl,

    /// Whether this this chain is already running or should be started
    /// (applies only if `url` is a loopback address).
    #[serde(default = "default_true")]
    pub autostart: bool,

    /// Whether to run a local credentials proxy in front of the endpoint
    /// (applies only if `url` is a remote address).
    pub proxy: Option<ProxyConfig>,
}

/// Subnet information.
#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct SubnetInfo {
    /// Arbitrary VM name
    pub vm_name: String,
    /// VM id, **must be derived** from 'vm_name' (TODO: compute this field)
    pub vm_id: String,
    /// Chain ID, must be unique across all chains.
    pub chain_id: u32,
    /// Blockchain id, **must be derived** from everything else
    pub blockchain_id: String,
}

impl Default for SubnetInfo {
    fn default() -> Self {
        Self {
            vm_name: "cubisttestsubnet".into(),
            vm_id: "koY1rHkeQ4E8mLjxQwVmq93e7F9utQejpSVFidEqZfFmGrWQ1".into(),
            chain_id: 23456,
            blockchain_id: "2FQZ2GMqphsQ8jpXa6ttFYBHZwso7LstGTDigQHPDh3ySHySNV".into(),
        }
    }
}

/// A config for avalanche endpoints
#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AvalancheConfig {
    /// Config options shared with all configs
    #[serde(flatten)]
    pub common: CommonConfig,
    /// Number of nodes in the network (min 4)
    #[serde(default = "default_ava_nodes")]
    pub num_nodes: u16,
    /// Optional subnets to create
    #[serde(default)]
    pub subnets: Vec<SubnetInfo>,
}

impl AvalancheConfig {
    /// Returns a new `AvalancheConfig` instance whose `subnets` field
    /// is set to the subnets value of `self` if not empty or a
    /// singleton vector containing a default [`SubnetInfo`].
    pub fn with_default_subnet(self) -> Self {
        if self.subnets.is_empty() {
            AvalancheConfig {
                subnets: vec![Default::default()],
                ..self
            }
        } else {
            self
        }
    }

    const DEFAULT_AVALANCHE_CHAIN_ID: u32 = 43112;

    /// Returns ethereum RPC endpoint relative path
    pub fn eth_endpoint_and_chain_id(&self) -> (String, u32) {
        // TODO: why first if there are more than one???
        let (blockchain_id, chain_id) = if let Some(sub) = self.subnets.first() {
            (sub.blockchain_id.as_str(), sub.chain_id)
        } else {
            ("C", Self::DEFAULT_AVALANCHE_CHAIN_ID)
        };
        (format!("ext/bc/{blockchain_id}/rpc"), chain_id)
    }
}

/// A config for polygon endpoints
#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PolygonConfig {
    /// Config options shared with all configs
    #[serde(flatten)]
    pub common: CommonConfig,
    /// Accounts to generate and fund for local testnet
    #[serde(default = "default_local_accounts")]
    pub local_accounts: Vec<CredConfig>,
}

/// Configuration for ethereum endpoints
#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EthereumConfig {
    /// The set of config options shared by all endpoints
    #[serde(flatten)]
    pub common: CommonConfig,
    /// Accounts to generate and fund for local testnet
    #[serde(default = "default_mnemonic_config")]
    pub bootstrap_mnemonic: MnemonicConfig,
}

/// The default [derivation path](https://github.com/bitcoin/bips/blob/master/bip-0044.mediawiki)
/// prefix used by Ethereum. This value is used when `EthereumConfig::derivation_path` is `None`.
pub const DEFAULT_ETH_DERIVATION_PATH_PREFIX: &str = "m/44'/60'/0'/0/";

fn default_derivation_path() -> String {
    DEFAULT_ETH_DERIVATION_PATH_PREFIX.into()
}

fn default_mnemonic() -> String {
    "test test test test test test test test test test test junk".into()
}

fn one() -> u16 {
    1
}

fn default_ava_nodes() -> u16 {
    5
}

fn default_true() -> bool {
    true
}

fn default_mnemonic_config() -> MnemonicConfig {
    MnemonicConfig {
        seed: default_mnemonic().into(),
        account_count: 1,
        derivation_path: default_derivation_path(),
    }
}

fn default_local_accounts() -> Vec<CredConfig> {
    vec![CredConfig::Mnemonic(default_mnemonic_config())]
}

#[cfg(test)]
mod test {
    use coins_bip39::{English, Mnemonic};
    use serde_json::json;

    use crate::secret::{
        SecretKind, INVALID_MNEMONIC_ERR, INVALID_PRIVATE_KEY_ERR, INVALID_PRIVATE_KEY_HEX_ERR,
    };

    use super::{MnemonicConfig, PolygonConfig, PrivateKeyConfig, ProxyConfig};
    use secrecy::ExposeSecret;

    #[test]
    fn serde_mnemonic_valid() {
        let m: String = Mnemonic::<English>::new(&mut rand::thread_rng())
            .to_phrase()
            .unwrap();
        let json = json!({ "seed": { "secret": m } });
        let mc: MnemonicConfig = serde_json::from_value(json).unwrap();
        let sec = mc.seed.load().unwrap();
        assert_eq!(&m, sec.expose_secret());
    }

    #[test]
    fn serde_mnemonic_invalid() {
        let m: String = "blah blah truc".into();
        let json = json!({ "seed": { "secret": m } });
        match serde_json::from_value::<MnemonicConfig>(json) {
            Ok(_) => panic!("String '{m}' is not a valid bip39 phrase and thus should be rejected"),
            Err(e) => assert!(e.to_string().contains(INVALID_MNEMONIC_ERR), "{e}"),
        };
    }

    #[test]
    fn serde_proxy_config_invalid_mnemonic() {
        let m: String = "blah blah truc".into();
        let json = json!({
            "port": 12345,
            "creds": [{ "mnemonic": { "seed": { "secret": m } } }],
            "chain_id": 23456,
        });
        match serde_json::from_value::<ProxyConfig>(json) {
            Ok(_) => panic!("String '{m}' is not a valid bip39 phrase and thus should be rejected"),
            Err(e) => assert!(e.to_string().contains(INVALID_MNEMONIC_ERR), "{e}"),
        };
    }

    #[test]
    fn serde_polygon_config_invalid_mnemonic() {
        let m: String = "blah blah truc".into();
        let json = json!({
            "url": "http://localhost:12345",
            "local_accounts": [{ "mnemonic": { "seed": { "secret": m } } }],
        });
        match serde_json::from_value::<PolygonConfig>(json) {
            Ok(_) => panic!("String '{m}' is not a valid bip39 phrase and thus should be rejected"),
            Err(e) => assert!(e.to_string().contains(INVALID_MNEMONIC_ERR), "{e}"),
        };
    }

    #[test]
    fn serde_mnemonic_cannot_load() {
        let json = json!({ "seed": { "env": "missing_env_var_123_not_found" } });
        // doesn't eagerly fail if secret is not set at all
        let mc: MnemonicConfig = serde_json::from_value(json).unwrap();
        assert!(matches!(mc.seed.inner, SecretKind::EnvVar { .. }));
    }

    #[test]
    fn serde_private_key_invalid_hex() {
        let key: String = "blah blah truc".into();
        let json = json!({ "hex": { "secret": key } });
        match serde_json::from_value::<PrivateKeyConfig>(json) {
            Ok(_) => panic!("String '{key}' is not a valid hex string and thus should be rejected"),
            Err(e) => assert!(e.to_string().contains(INVALID_PRIVATE_KEY_HEX_ERR)),
        };
    }

    #[test]
    fn serde_private_key_invalid_k2561() {
        let key: String = "FEEDF00D".into();
        let json = json!({ "hex": { "secret": key } });
        match serde_json::from_value::<PrivateKeyConfig>(json) {
            Ok(_) => panic!("String '{key}' is not a valid K-256 key and thus should be rejected"),
            Err(e) => assert!(e.to_string().contains(INVALID_PRIVATE_KEY_ERR)),
        };
    }

    #[test]
    fn serde_private_key_valid_k2561() {
        let key = "56289e99c94b6912bfc12adc093c9b51124f0dc54ac7a766b2bc5ccf558d8027";
        let json = json!({ "hex": { "secret": key } });
        let pk = serde_json::from_value::<PrivateKeyConfig>(json).unwrap();
        assert!(matches!(pk.hex.inner, SecretKind::PlainText { .. }));
        let loaded = pk.hex.load().unwrap();
        assert_eq!(key, loaded.expose_secret());
    }
}
