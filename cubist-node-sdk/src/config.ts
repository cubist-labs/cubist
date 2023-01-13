/** Port of the Cubist Rust SDK Config to TypeScript. */
import * as fs from 'fs';
import * as path from 'path';
import { sync as globSync, } from 'fast-glob';
import { URL, } from 'url';
import { cwd, } from 'process';
import { ConfigError, } from './config/errors';
import { NetworkProfile, EndpointConfig, } from './config/network';
import { validateConfig, } from './config/schema/validator';
import { find_file, } from './utils';
import * as typechain from 'typechain';
import { setDefaultResultOrder, } from 'dns';
import { PathBuf, } from './config/pre_compile_manifest';

export * from './config/errors';
export * from './config/pre_compile_manifest';
export * from './config/network';

/** @internal Error raised when we can't find the config file */
export class FileNotFound extends ConfigError {
  /** Can't find the config file.
   * @param {PathBuf?} file - The file we couldn't find (or default config file).
   */
  constructor(file?: PathBuf) {
    super(`Could not find config file ${file || Config.DEFAULT_FILENAME}`);
    this.name = 'FileNotFound';
  }
}

/** @internal Error raised when contract files are not within root directory. */
export class InvalidContractFilePaths extends ConfigError {
  /** Invalid contract file paths.
   * @param {PathBuf[]} paths - Paths that are not within the root directory.
   */
  constructor(paths: PathBuf[]) {
    super(`Contract source files outside root directory: ${paths}`);
    this.name = 'InvalidContractFilePaths';
  }
}

/** @internal Error raised when a target has no matching files. */
export class NoFilesForTarget extends ConfigError {
  /** No files for target.
    * @param {Target} target - The target that has no files missing.
    */
  constructor(target: Target) {
    super(`No files for target ${target}`);
    this.name = 'NoFilesForTarget';
  }
}

/** @internal Error raised when `network_profile` is specified but
 * no such network profile is defined under `network_profiles`. */
export class MissingNetworkProfile extends ConfigError {
  /** Missing network profile.
   * @param {NetworkProfileName} profile - The profile that is missing.
   */
  constructor(profile: NetworkProfileName) {
    super(`Specified network profile ('${profile}') not found`);
    this.name = 'MissingNetworkProfile';
  }
}

/**
 * Type alias for "network profile name" to be used in hash maps.
 *
 * @group Internal
 * */
export type NetworkProfileName = string;

/** Project type.
 *
 * @group Internal
 * */
export enum ProjType {
  /**  JavaScript */
  JavaScript = 'JavaScript', // eslint-disable-line no-unused-vars
  /**  TypeScript */
  TypeScript = 'TypeScript', // eslint-disable-line no-unused-vars
  /**  Rust */
  Rust = 'Rust', // eslint-disable-line no-unused-vars
}

/**
 * The compiler used for compiling contract code. For now only `solc`.
 *
 * @group Internal
 * */
export enum Compiler {
  /**  Compile with the solc compiler. */
  Solc = 'solc', // eslint-disable-line no-unused-vars
  /**  Compile with the solang compiler.
  * @ignore
  * TODO: remove
  * */
  Solang = 'solang', // eslint-disable-line no-unused-vars
}

/** Target chains (e.g., Avalanche, Polygon, Ethereum) for which we can deploy
 * contracts.
 *
 * @group Core
 * */
export enum Target {
  /**  The avalanche chain */
  Avalanche = 'avalanche', // eslint-disable-line no-unused-vars
  /**  The polygon chain */
  Polygon = 'polygon', // eslint-disable-line no-unused-vars
  /**  The ethereum chain */
  Ethereum = 'ethereum', // eslint-disable-line no-unused-vars
  /** The avalanche subnet chain */
  AvaSubnet = 'ava_subnet', // eslint-disable-line no-unused-vars
}

/** Target configuration.
 *
 * @group Internal
 * */
export interface TargetConfig {
  /**  List of source files (after we resolve globs). */
  files: PathBuf[];
  /**  Compiler to compile the contract with. */
  compiler: Compiler;
}

/**
 * Contract configuration.
 *
 * @group Internal
 * */
export class ContractsConfig {
  /**  Root directory for contracts. */
  root_dir: PathBuf;
  /**  Target chain. */
  targets: Map<Target, TargetConfig>;
  /** Paths to search for imports. */
  import_dirs: PathBuf[];
  /** Paths relative to the root directory.
   * @param {PathBuf} p path to resolve relative to the root.
   * @return {PathBuf} resolved path.
   * @throws {InvalidContractFilePaths} if the path is not within the root directory.
   */
  relative_to_root(p: PathBuf): PathBuf {
    // / If p is under root_dir, returns its relative path (strip prefix).
    if (p.startsWith(this.root_dir)) {
      return path.relative(this.root_dir, p);
    }
    throw new InvalidContractFilePaths([p]);
  }
}

/** @internal Top-level cubist application configuration. */
export interface IConfig {
  /**  Project type */
  type: ProjType;
  /**  Path to the build directory. */
  build_dir: PathBuf;
  /**  Path to the deploy directory. */
  deploy_dir: PathBuf;
  /**  Contract configurations. */
  contracts: ContractsConfig,
  /** A map of named network profiles for use in development, testing, etc. */
  network_profiles: { [profile: NetworkProfileName]: NetworkProfile },
  /**  Selected network profile.  If omitted, defaults to "default". A network
  * profile with the same name must be defined in `network_profiles`. */
  current_network_profile: NetworkProfileName,
  /** Allows or disables imports from external sources (GitHub and npm/Yarn). */
  allow_import_from_external: boolean,
}

/**
 * Class that exposes Cubist project configurations (and resolves and validates
 * path names, network configurations, etc.).
 *
 * All cubist applications have a JSON config file ({@link DEFAULT_FILENAME}),
 * which specifies the kind of project ({@link ProjType}]) the application code
 * is written in, where build output should be written to, and where deployment
 * scripts and information should be generated.
 *
 * Example configuration file:
 * ``` json
 * {
 *   "type": "TypeScript",
 *   "build_dir": "build",
 *   "deploy_dir": "deploy",
 *   "contracts": {
 *     "root_dir": "contracts",
 *     "targets": {
 *       "ethereum" : {
 *         "files": ["./contracts/StorageReceiver.sol"]
 *       },
 *       "polygon": {
 *         "files": ["./contracts/StorageSender.sol"]
 *       }
 *     },
 *     "import_dirs": [
 *       "node_modules"
 *     ]
 *   },
 *   "allow_import_from_external": true,
 *   "network_profiles": {
 *       "default": {
 *           "ethereum": { "url": "http://127.0.0.1:8545/" },
 *           "polygon":  { "url": "http://127.0.0.1:9545" }
 *       }
 *   }
 * }
 * ```
 *
 * You can load config files with {@link nearest}, which finds the JSON file in
 * the current directory or any parent directory:
 *
 * ```
 * const cfg = Config.nearest();
 * ```
 * Alternatively, you can load the default config in the directory with
 * {@link from_dir}:
 * ```
 * const cfg = Config.from_dir("/path/to/my-app");
 * ```
 *
 * Alternatively, you can just use {@link from_file} if you have the filename
 * of the config file:
 *
 * ```
 * const cfg = Config.from_file("/path/to/cubist-config.json");
 * ```
 *
 * This class exposes a subset of the functionality available in our Rust SDK.
 * In particular, this class is only intended to be used to read configurations
 * from the file system. This means that every Config object should be treated as
 * effectively read-only.
 *
 * **NOTE**: Most users don't need to use this class; the class you likely want
 * to use is {@link Cubist}, which transparently loads configurations.
 *
 * @group Internal
 * */
export class Config {
  // The underlying configuration object.
  private json: IConfig;

  // Default cubist config filename
  static readonly DEFAULT_FILENAME = 'cubist-config.json';

  /** Absolute path to the file corresponding to this configuration.
  * @ignore */
  readonly config_path: string;

  /**  Project type */
  get type(): ProjType {
    return this._type;
  }
  private _type: ProjType;

  /** Selected network profile.  If omitted, defaults to "default". A network
  * profile with the same name must be defined in `network_profiles`. */
  get current_network_profile() : NetworkProfileName {
    return this._current_network_profile;
  }
  private _current_network_profile: NetworkProfileName;

  /** Allows or disables imports from external sources (GitHub and npm/Yarn). */
  get allow_import_from_external(): boolean {
    return this._allow_import_from_external;
  }
  private _allow_import_from_external: boolean;

  /** @ignore Empty constructor */
  private constructor() {
    // @eslint-disable-line @typescript-eslint/no-empty-function
  }

  /** Create configuration from config file in the current directory or some
   * parent directory.
   * @return {Config} the configuration.
   */
  static nearest(): Config {
    return Config.from_dir(cwd());
  }

  /** Create configuration from directory (using default filename).
   * @param {PathBuf} dir the directory.
   * @return {Config} the configuration.
   */
  static from_dir(dir: PathBuf): Config {
    return Config.from_file(path.join(dir, Config.DEFAULT_FILENAME));
  }

  /**
   * Create configuration from JSON file.
   * @param {PathBuf} config_path Path to the configuration file.
   * @return {Config} the configuration.
   */
  static from_file(config_path: PathBuf): Config {
    // Read the config file
    const json = JSON.parse(fs.readFileSync(config_path, 'utf8'));
    return Config._from_json(json, path.resolve(config_path));
  }

  /** Get the absolute project directory.
   * @return {PathBuf} the absolute project directory.
   */
  project_dir(): PathBuf {
    return path.dirname(this.config_path);
  }

  /** Get the absolute deploy directory.
   * @return {PathBuf} the absolute deploy directory.
   */
  deploy_dir(): PathBuf {
    return this.relative_to_project(this.json.deploy_dir);
  }

  /** Set the build directory. This function is for internal-use only. We use
   * it in the TestDK when we create a Cubist project with a temporary build
   * directory.
   * @internal
   * @arg {PathBuf} dir - the deploy directory.
   */
  __set_build_dir(dir: PathBuf) {
    this.json.build_dir = dir;
  }

  /** Set the deploy directory. This function is for internal-use only. We use
   * it in the TestDK when we create a Cubist project with a temporary deploy
   * directory.
   * @internal
   * @arg {PathBuf} dir - the deploy directory.
   */
  __set_deploy_dir(dir: PathBuf) {
    this.json.deploy_dir = dir;
  }

  /** Get the absolute build directory.
   * @return {PathBuf} the absolute build directory.
   */
  build_dir(): PathBuf {
    return this.relative_to_project(this.json.build_dir);
  }

  /** Return path relative to the project root (if not absolute)
   * @param {PathBuf} file the path to make relative to the project directory.
   * @return {PathBuf} the absolute path.
   */
  private relative_to_project(file: PathBuf): PathBuf {
    const p = path.isAbsolute(file) ? file : path.join(this.project_dir(), file);
    return path.normalize(p);
  }

  /** Get contracts config with paths "canonicalized" (cleaned up and absolute).
   * @return {ContractsConfig} the contracts config.
   */
  contracts(): ContractsConfig {
    return this.json.contracts;
  }

  /**
  * Return all targets.
  * @return {Target[]} the targets.
  * */
  targets(): Target[] {
    return Array.from(this.json.contracts.targets.keys());
  }

  /** Return configured network (if any) for a given target.
    * @param {Target} target the target name.
    * @return {EndpointConfig|undefined} the network config if it exists.
    */
  network_for_target(target: Target): EndpointConfig | undefined {
    return this.network_for_target_in_profile(target, this.current_network_profile);
  }

  /** Return configured network (if any) for a given target in a given profile.
   * @param {Target} target the target name.
   * @param {NetworkProfileName} profile_name the network profile name.
   * @return {EndpointConfig|undefined} the network config if it exists.
   */
  private network_for_target_in_profile(target: Target, profile_name:
                                        NetworkProfileName): EndpointConfig |
                                        undefined {
    const profile = this.json.network_profiles[profile_name];
    return profile ? profile[target] : undefined;
  }

  /** Return the currently selected network profile.
  * @return {NetworkProfile} the current network profile.
  **/
  network_profile(): NetworkProfile {
    return this.json.network_profiles[this.current_network_profile];
  }

  /** Return the network profile by name.
   * @param {string} name profile name
   * @return {NetworkProfile} corresponding to a given name.
   **/
  network_profile_by_name(name: NetworkProfileName): NetworkProfile {
    return this.json.network_profiles[name];
  }

  /**
   * Generate TypeScript types for the configuration.
   */
  async generate_types(): Promise<void> {
    for (const target of this.targets()) {
      const build_dir = path.join(this.build_dir(), target);
      const outDir = path.join(build_dir, 'types');
      const allFiles = globSync('artifacts/*.sol/*.json', { cwd: build_dir, }).
        map((f) => path.join(build_dir, f));
      await typechain.runTypeChain({
        cwd: build_dir,
        filesToProcess: allFiles,
        allFiles,
        outDir,
        target: 'ethers-v5',
      });
    }
  }


  /** **********************************************************************
   * Internals                                                             *
   *************************************************************************/

  /**
   * Create configuration from JSON object.
   *
   * This is used for internally and testing. This function is not exposed in
   * the Rust SDK; it might go away from the JS SDK as well.
   *
   * @param {IConfig} json the configuration object.
   * @param {PathBuf} config_path the path to the config file (potentially nonexistant yet).
   * @return {Config} the configuration.
   * @internal
   */
  static _from_json(json: IConfig, config_path: PathBuf): Config {
    const self = new Config();

    // Set the underlying JSON
    self.json = json;

    // Validate the json against the schema
    validateConfig(self.json);

    // Set stuff from self.json
    self._type = self.json.type;
    self._current_network_profile = self.json.current_network_profile;
    self._allow_import_from_external = self.json.allow_import_from_external;

    // Set the config path to the real path
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    (self as any /* to bypass read-only */).config_path = config_path;

    // Update the underlying JSON with typed values
    // 1. typed contracts config and resolve globs
    self.typeContractsConfig();
    // 2. typed networks
    self.typeNetworks();

    // Validate config
    self.validate();

    // Set resolver prerference to ipv4
    self.setDNSPriorityToIPv4ifLocalhost();

    return self;
  }

  /** Type the underlying json contracts config and make sure that paths are
   * "canonicalized" (cleaned up and absolute). When we read the JSON config,
   * we don't automatically parse the contracts into a typed ContractsConfig;
   * this functions does this and canonicalizes the paths. This function should
   * be called in the constructor, once. */
  private typeContractsConfig() {
    const contracts = new ContractsConfig();
    contracts.root_dir = this.relative_to_project(this.json.contracts.root_dir);
    contracts.import_dirs = this.json.contracts.import_dirs.map((d) => this.relative_to_project(d));
    contracts.targets = new Map();

    // Normalize each contract directory
    for (const [target, target_config] of Object.entries(this.json.contracts.targets)) {
      const files = [];
      // resolve globs
      target_config.files.forEach((glob: string) => {
        globSync(glob, { cwd: this.project_dir(), }).forEach((file: string) => {
          files.push(this.relative_to_project(file));
        });
      });
      contracts.targets.set(target as Target, {
        files,
        compiler: target_config.compiler || Compiler.Solc,
      });
    }
    this.json.contracts = contracts;
  }

  /** Same as typeContractsConfig, but for networks. */
  private typeNetworks() {
    // Update the url (NetworkProfile is an interface not class) to be a URL
    for (const network_profile of Object.values(this.json.network_profiles)) {
      for (const endpoint_config of Object.values(network_profile)) {
        endpoint_config.url = new URL((endpoint_config.url as unknown) as string);
      }
    }
  }

  /** Node's resolver as of v17 prefers ipv6; for localhost, our cubist cli
   * binds to ipv4. Set the resolver to prefer ipv4 if any of the URLs are
  * localhost. */
  private setDNSPriorityToIPv4ifLocalhost() {
    for (const network_profile of Object.values(this.json.network_profiles)) {
      for (const endpoint_config of Object.values(network_profile)) {
        if (endpoint_config.url.hostname === 'localhost') {
          setDefaultResultOrder('ipv4first');
          return;
        }
      }
    }
  }

  /** Check that the config is valid. */
  private validate() {
    // Validate paths
    this.validate_contract_paths();
    // Validate network profiles
    this.validate_network_profiles();
  }

  /** Check if the contracts config is valid. */
  private validate_contract_paths() {
    // make sure every target_config file is in root_dir
    const contracts = this.contracts();
    const root_dir = contracts.root_dir;
    const bad_paths = [];
    for (const [target, target_config] of contracts.targets) {
      if (target_config.files.length === 0) {
        throw new NoFilesForTarget(target);
      }
      for (const file of target_config.files) {
        if (!file.startsWith(root_dir)) {
          bad_paths.push(file);
        }
      }
    }
    if (bad_paths.length > 0) {
      throw new InvalidContractFilePaths(bad_paths);
    }
  }

  /** Check if targets point to valid (defined) networks. */
  private validate_network_profiles() {
    // network profile is valid
    if (this.current_network_profile !== 'default' &&
      !(this.current_network_profile in this.json.network_profiles)) {
      throw new MissingNetworkProfile(this.current_network_profile);
    }
  }
}

/**
 * Exports for tests
 * @ignore
 */
export const _testExports = {
  find_file,
  network_for_target_in_profile:
    (Config.prototype as any). // eslint-disable-line @typescript-eslint/no-explicit-any
      network_for_target_in_profile,
};
