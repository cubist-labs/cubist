import { ContractName, } from '../../config';
import { TargetProject, BigNumber, } from '../';
import {
  NamedContract as NamedSolidityContract,
  Contract as SolidityContract,
  Address as SolidityAddress,
} from './solidity';
import { ContractFQN, } from '../..';


/** Contract address (string for now). */
export type ContractAddress = SolidityAddress;

/** Account address (string for now). */
export type AccountAddress = SolidityAddress;

/** Contracts are (for now) just ethers.js'
* [Contract](https://docs.ethers.org/v5/api/contract/contract/)s */
export type Contract = SolidityContract;

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
   * @return {Promise<Address[]>} Accounts.
   */
  abstract accounts(): Promise<AccountAddress[]>;

  /** Return default signer address.
   * @return {Promise<Address>} Default signer address. */
  abstract getSignerAddress(): Promise<AccountAddress>;


  /** Get the balance of the given address.
    * @param {Address} addr - The address.
    * @return {Promise<BigNumber>} The balance. */
  abstract getBalance(addr: AccountAddress): Promise<BigNumber>;
}
