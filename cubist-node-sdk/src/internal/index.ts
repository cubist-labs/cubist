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
export { BigNumber, } from './target_handler/solidity';

export {
  ContractFactory,
  Contract,
  ContractAddress,
  AccountAddress,
} from './target_handler/';

/**
 * Project encapsulating all contracts and contract factories for a particular
 * target chain.
 */
export class TargetProject {
  public readonly config: Config;
  private readonly _target: Target;
  private readonly handler: TargetProjectHandler;

  /** Create new project per target
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

  /** Get contract factory. This just calls the underlying project handlers.
  * @param {ContractName} name - The contract name.
  * @return {ContractFactory} The contract factory.
  * */
  getContractFactory(name: ContractName): ContractFactory {
    return this.handler.getContractFactory(name);
  }

  /** Get deployed contract.
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

  /**
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

  /** Return default signer address.
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
 * Generate TypeScript types for given project (or nearest).
 * You don't generally need to call this directly. The `cubist build` command
 * does it for you (via the `cubist gen` command).
 * @param {string?} file - Optional config file path; otherwise, use nearest config.
 */
export async function genTypes(file?: string) : Promise<void> {
  const cfg = file ? Config.from_file(file) : Config.nearest();
  if (cfg.type === ProjType.TypeScript) {
    console.log('Generating TypeScript types...');
    await cfg.generate_types();
  }
}
