import { URL, } from 'url';
import { PathBuf, } from './pre_compile_manifest';

/** The configuration for a suite of endpoints. Used to specify a single or
 * multi-chain environment.
 *
 * @group Internal
 * */
export interface NetworkProfile {
    /** configuration for an ethereum endpoint */
    ethereum?: EthereumConfig,
    /** configuration for an avalanche endpoint */
    avalanche?: AvalancheConfig,
    /** configuration for a polygon endpoint */
    polygon?: PolygonConfig,
    /** configuration for a avalanche subnet endpoint */
    ava_subnet?: AvalancheConfig,
}

/**
 * Configuration for an unspecified network.
 *
 * @group Internal
 * */
export type EndpointConfig = EthereumConfig | AvalancheConfig | PolygonConfig;

/** Contains the config options that are common to all providers.
 *
 * @group Internal
 * */
export interface CommonConfig {
    /** Url the endpoint can be found at */
    url: URL,
    /** Whether this this chain is already running or should be started
     * (applies only if `url` is a loopback address). */
    autostart: boolean,
    /** Whether to run a credentials proxy in front of the endpoint
     * (applies only if `url` is a remote address). */
    proxy: ProxyConfig,
}

/** Proxy configuration.
 *
 * @group Internal
 * */
export interface ProxyConfig {
    /** Local port where the proxy will run */
    port: number,
    /** @internal Credentials configuration */
    creds: CredConfig[],
    /** Chain id (transaction chain ID must be set before signing) */
    chain_id: number,
}

/** @internal Credential config. */
export interface CredConfig {
    mnemonic?: MnemonicConfig,
    keystore?: KeystoreConfig,
    private_key?: PrivateKeyConfig,
}

/** @internal Different kinds of secrets cubist supports. */
export type Secret = PlainTextSecret | EnvVarSecret | FileSecret;

/** @internal Secret saved as plain text */
export interface PlainTextSecret {
    /** The secret value */
    secret: string,
}

/** @internal Secret value is the value of an environment variable.
 * If found, .env file is automatically loaded. */
export interface EnvVarSecret {
    /** Name of the environment variable */
    env: string,
}

/** @internal Secret value is the contents of a file */
export interface FileSecret {
    /** File path */
    file: string,
}

/** @internal Configuration for mnemonic-based credentials */
export interface MnemonicConfig {
    /** The bip39 english string used as the seed for generating accounts */
    seed: Secret,
    /** The number of accounts to generate using the mnemonic */
    account_count: number,
    /** The derivation path, or None for the default `m/44’/60’/0’/0/` */
    derivation_path: string,
}

/** @internal Configuration for keystore-based credentials */
export interface KeystoreConfig {
    /** Encrypted keystore */
    file: PathBuf,
    /** Password for decrypting the keystore */
    password: Secret,
}

/** @internal Configuration for private key-based credentials */
export interface PrivateKeyConfig {
    /** Hex-encoded private key (should not start with "0x") */
    hex: Secret,
}

/** A config for avalanche endpoints.
 *
 * @group Internal
 * */
// eslint-disable-next-line @typescript-eslint/no-empty-interface
export interface AvalancheConfig extends CommonConfig {
    num_nodes: number,
    subnets: SubnetInfo[],
}

/** Subnet info.
 *
 * @group Internal
 * */
export interface SubnetInfo {
    // Arbitrary VM name
    vm_name: string,
    // VM id, **must be derived** from 'vm_name' (TODO: compute this field)
    vm_id: string,
    // Chain ID, must be unique across all chains.
    chain_id: number,
    // Blockchain id, **must be derived** from everything else
    blockchain_id: string,
}

/** A config for polygon endpoints
 *
 * @group Internal
 * */
// eslint-disable-next-line @typescript-eslint/no-empty-interface
export interface PolygonConfig extends CommonConfig { }

/** A config for ethereum endpoints.
 *
 * @group Internal
 * */
export interface EthereumConfig extends CommonConfig {
    /** A mnemonic to use when generating accounts for local endpoints */
    mnemonic: string,
    /** Number of accounts to generate for local deployment */
    number_of_accounts: number,
}
