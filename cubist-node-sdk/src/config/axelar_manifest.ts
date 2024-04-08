import * as fs from 'fs';
import { validateAxelarManifest, } from './schema/validator';
import { PathBuf, } from '../config';
import { Address, } from '../internal/target_handler/solidity';

/**
 * Per-target manifest file that the Axelar relayer produces.
 *
 * @group Internal
 */
export interface IAxelarManifest {
    /** Chain name */
    name: string,
    /** Chain id */
    chainId: number,
    /** Gateway contract address */
    gateway: Address,
    /** Gas receiver contract address */
    gasReceiver: Address,
    /** Deployer contract address */
    constAddressDeployer: Address,
}

/**
 * Per-target manifest file that the Axelar relayer produces.
 *
 * @group Internal
 */
export class AxelarManifest {
  private _name: string;
  private _chainId: number;
  private _gateway: string;
  private _gasReceiver: string;
  private _constAddressDeployer: string;

  /** Chain target */
  get name(): string {
    return this._name;
  }

  /** Chain id */
  get chainId(): number {
    return this._chainId;
  }

  /** Gateway contract address */
  get gateway(): Address {
    return this._gateway;
  }

  /** Gas receiver contract address */
  get gasReceiver(): Address {
    return this._gasReceiver;
  }

  /** Deployer contract address */
  get constAddressDeployer(): Address {
    return this._constAddressDeployer;
  }

  /** @ignore Empty constructor */
  private constructor() {
    // @eslint-disable-line @typescript-eslint/no-empty-function
  }

  /**
   * Create manifest from JSON file.
   * @param {PathBuf} file Path to the manifest file.
   * @return {AxelarManifest} the manifest.
   */
  static from_file(file: PathBuf): AxelarManifest {
    const json = JSON.parse(fs.readFileSync(file, 'utf8'));
    return AxelarManifest._from_json(json);
  }

  /**
   * Create manifest from JSON object.
   *
   * @param {IAxelarManifest} json the manifest object.
   * @return {AxelarManifest} the manifest.
   * @internal
   */
  static _from_json(json: IAxelarManifest): AxelarManifest {
    const self = new AxelarManifest();

    // Validate the json against the schema
    validateAxelarManifest(json);

    // Populate the object
    self._name = json.name;
    self._chainId = json.chainId;
    self._constAddressDeployer = json.constAddressDeployer;
    self._gasReceiver = json.gasReceiver;
    self._gateway = json.gateway;

    return self;
  }
}
