/**
 * This module exports the {@link TargetProject} class, which abstracts over
 * single-chain projects, and aliases we export from
 * [ethers.js](https://docs.ethers.io/v5/) (e.g., {@link Contract}s and
 * {@link BigNumber}s).
 *
 * This module is internal to Cubist and is likely to change in the future.
 *
 * @module
 */

import {
  Compiler,
  Config,
  ContractName,
  ProjType,
  Target,
} from '../config';
import {
  TargetProjectHandler,
  ContractFactory,
  Contract,
  ContractAddress,
  AccountAddress,
  NamedContract,
} from './target_handler/';

import * as solidity from './target_handler/solidity';
export * as solidity from './target_handler/solidity';

import { BigNumber, } from './target_handler/solidity';
/** ethers.js' BigNumber */
export { BigNumber, } from './target_handler/solidity';

export {
  ContractFactory,
  Contract,
  ContractAddress,
  AccountAddress,
} from './target_handler/';

/**
 * Project encapsulating all contracts and contract factories for a particular
 * target chain. This class largely abstracts over any particular chain
 * details; instead all the hard work is done by `TargetProjectHandler`s. We
 * currently only have a handler for Solidity projects, but we plan to add
 * support for other languages in the future and will make the handler
 * interface public so the community can add handlers to different kinds of
 * projects.
 *
 * This class is largley internal and will likely not be exposed in future
 * versions. The only user-facing methods for now are for getting information
 * from the project node (e.g., accounts, balances, etc.).
 *
 * @group Advanced
 */
export class TargetProject {
  /** Underlying config */
  public readonly config: Config;
  private readonly _target: Target;
  private readonly handler: TargetProjectHandler;

  /** @internal Create new project per target
   * @param {Target} target - The target chain
   * @param {Config?} config - Optional config (using near otherwise).
   */
  constructor(target: Target, config?: Config) {
    this._target = target;
    this.config = config ?? Config.nearest();

    const target_config = this.config.contracts().targets.get(target);
    if (!target_config) {
      throw new Error(`Target '${target}' not found in config`);
    }
    if (target_config.compiler == Compiler.Solc) {
      this.handler = new solidity.Handler(this);
    } else {
      throw new Error(`Unsupported '${target_config.compiler}' projects`);
    }
  }

  /** @return {Target} - The target chain */
  public target(): Target {
    return this._target;
  }

  /** @internal Get contract factory.
   * @param {ContractName} name - The contract name.
   * @return {ContractFactory} The contract factory.
   * */
  getContractFactory(name: ContractName): ContractFactory {
    return this.handler.getContractFactory(name);
  }

  /** @internal Get deployed contract.
   * @param {ContractName} name - The contract name.
   * @param {ContractAddress?} addr - Optional contract address (if more than
   * one contract with same name).
   * @param {boolean} ignoreReceipt - Ignore receipt (e.g., if contract deployed
   * with another tool).
   * @return {Contract} The contract.
   * @throws {Error} If the contract could not be found, if there are multiple
   * contracts and the address argument is omitted, or if the receipt is missing
   * (unless ignoreReceipt is set).
   * */
  getContract(name: ContractName, addr?: ContractAddress, ignoreReceipt = false): Contract {
    return this.getNamedContract(name, addr, ignoreReceipt).inner;
  }

  /** @internal Get deployed named contract.
    * @param {ContractName} name - The contract name.
    * @param {ContractAddress?} addr - Optional contract address (if more than
    * one contract with same name).
    * @param {boolean} ignoreRec - Ignore receipt (e.g., if contract deployed
    * with another tool).
    * @return {NamedContract} The contract.
    * @throws {Error} If the contract could not be found, if there are multiple
    * contracts and the address argument is omitted, or if the receipt is missing
    * (unless ignoreReceipt is set).
    * */
  getNamedContract(name: ContractName, addr?: ContractAddress, ignoreRec = false): NamedContract {
    return this.handler.getNamedContract(name, addr, ignoreRec);
  }

  /** Check if contract has been deployed.
  * @param {ContractName} name - The contract name.
  * @return {boolean} true if contract was deployed at least once.
  * */
  isDeployed(name: ContractName): boolean {
    return this.handler.isDeployed(name);
  }

  /** Retrieve all accounts used on this target.
   * @return {Promise<Address[]>} Accounts.
   */
  accounts(): Promise<AccountAddress[]> {
    return this.handler.accounts();
  }

  /** Return default signer address (`accounts[0]`) for now.
   * @return {Promise<Address>} Default signer address. */
  getSignerAddress(): Promise<AccountAddress> {
    return this.handler.getSignerAddress();
  }

  /** Get the balance of the given address.
    * @param {Address} addr - The address.
    * @return {Promise<BigNumber>} The balance. */
  getBalance(addr: AccountAddress): Promise<BigNumber> {
    return this.handler.getBalance(addr);
  }
}

/**
 * @internal Generate TypeScript types for given project (or nearest). You
 * don't generally need to call this directly. The `cubist build` command does
 * it for you (via the `cubist gen` command).
 * @param {string?} file - Optional config file path; otherwise, use nearest config.
 */
export async function genTypes(file?: string) : Promise<void> {
  const cfg = file ? Config.from_file(file) : Config.nearest();
  if (cfg.type === ProjType.TypeScript) {
    console.log('Generating TypeScript types...');
    await cfg.generate_types();
  }
}
