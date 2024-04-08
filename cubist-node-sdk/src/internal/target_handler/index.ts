import { ContractName, } from '../../config';
import { TargetProject, BigNumber, } from '../';
import {
  NamedContract as NamedSolidityContract,
  Contract as SolidityContract,
  Address as SolidityAddress,
  Signer,
} from './solidity';
import { ContractFQN, } from '../..';


/** Contract address (string for now). */
export type ContractAddress = SolidityAddress;

/** Account address (string for now). */
export type AccountAddress = SolidityAddress;

/** An address is a contract or account address. */
export type Address = ContractAddress | AccountAddress;

/** Account address or account index. */
export type AccountAddressOrIndex = AccountAddress | number;

/** Contracts are (for now) just ethers.js'
* [Contract](https://docs.ethers.org/v5/api/contract/contract/)s */
export type Contract = SolidityContract;

/** @internal Ethers.js signer */
export { Signer, } from './solidity';

/** @internal Name contract */
export type NamedContract = NamedSolidityContract;

/**
 * @internal Contract factory interface that each Cubist target handler must
 * implement. This is deliberately simple for now.
 */
export interface ContractFactory {
  /** Get fully qualified contract name. */
  fqn(): ContractFQN;

  /** Contract factories can be used to deploy new contracts. */
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  deploy(...args: any[]): Promise<Contract>;
}

/**
 * @internal Abstract class for handling compiler-specific artifacts.
 */
export abstract class TargetProjectHandler {
  protected readonly project: TargetProject;

  /** Create new project handler.
   * @param {TargetProject} project - The project to handle.
   */
  constructor(project: TargetProject) {
    this.project = project;
  }

  /** Get contract factory.
   * @param {ContractName} name - The contract name.
   * @return {ContractFactory} The contract factory.
   * */
  abstract getContractFactory(name: ContractName): ContractFactory;

  /** Get deployed contract.
   * @param {ContractName} name - The contract name.
   * @param {ContractAddress?} addr - Optional contract address (if more than
   * one contract with same name).
   * @param {boolean} ignoreReceipt - Ignore receipt (e.g., if contract deployed
   * with another tool).
   * @return {NamedContract} The contract with its full name.
   * @throws {Error} If the contract could not be found, if there are multiple
   * contracts and the address argument is omitted, or if the receipt is missing
   * (unless ignoreReceipt is set).
   * */
  abstract getNamedContract(name: ContractName, addr?: ContractAddress,
                       ignoreReceipt?: boolean): NamedContract;

  /** Check if contract has been deployed.
  * @param {ContractName} name - The contract name.
  * @return {boolean} true if contract was deployed at least once.
  * */
  abstract isDeployed(name: ContractName): boolean;

  /** Utility function that calls {@link getNamedContract} and
   * returns its the inner contract.
   *
   * @param {ContractName} name - The contract name.
   * @param {ContractAddress?} addr - Optional contract address (if more than
   * one contract with same name).
   * @param {boolean} ignoreReceipt - Ignore receipt (e.g., if contract deployed
   * with another tool).
   * @return {Contract} The contract with its full name.
   * @throws {Error} If the contract could not be found, if there are multiple
   * contracts and the address argument is omitted, or if the receipt is missing
   * (unless ignoreReceipt is set).
   * */
  getContract(name: ContractName, addr?: ContractAddress, ignoreReceipt?: boolean): Contract {
    return this.getNamedContract(name, addr, ignoreReceipt).inner;
  }

  /** Retrieve all accounts used on this target.
   * @return {Promise<AccountAddress[]>} Accounts.
   */
  abstract accounts(): Promise<AccountAddress[]>;

  /** Get default signer we use for all transactions.
   * @return {Promise<AccountAddress>} The account address.
   */
  abstract getDefaultSignerAccount(): Promise<AccountAddress>;

  /** Set default signer we use for all transactions.
   * @param {AccountAddressOrIndex} addrOrIndex - Account address or index.
   */
  abstract setDefaultSignerAccount(addrOrIndex: AccountAddressOrIndex): Promise<void>;

  /** @internal Get the actual ethers.js signer for the current network profile.
   * @param {AccountAddressOrIndex?} addrOrIndex - Optional account address or index.
   * @return {Signer} The signer. */
  abstract getSigner(addrOrIndex?: AccountAddressOrIndex): Signer;

  /** Get the balance of the given address.
    * @param {Address} addr - The address.
    * @return {Promise<BigNumber>} The balance. */
  abstract getBalance(addr: Address): Promise<BigNumber>;
}
