import * as path from 'path';
import * as fs from 'fs';
import { mkdir, writeFile, } from 'fs/promises';
import { ethers, } from 'ethers';
import { TargetProject, } from '../';
import {
  TargetProjectHandler,
  ContractFactory as ProjectFactory,
} from './';
import {
  ContractName,
  EndpointConfig,
} from '../../config';
import { ContractFQN, } from '../..';

export type Address = string;
export type ContractInterface = ethers.ContractInterface;
export type BytesLike = ethers.BytesLike;
export type Signer = ethers.Signer;
export type ContractReceipt = ethers.ContractReceipt;
export type BigNumber = ethers.BigNumber;
export const BigNumber = ethers.BigNumber;

/** Type for custom save-receipt functions */
export type SaveReceiptFunction = (receipt: ContractReceipt) => Promise<void>;

/** Type alias for the internal ethers Contract type */
export type Contract = ethers.Contract;

/** Type that extends the internal ethers Contract type with a fully qualified name. */
export interface NamedContract {
  /** Fully qualified name */
  fqn: ContractFQN,
  /** Inner ethers contract */
  inner: Contract,
}

/** Contract factory optional arguments. */
export interface ContractFactoryOptions {
  signer?: Signer;
  saveReceipt?: SaveReceiptFunction;
}

/** Artifact type */
export interface Artifact {
  fqn: ContractFQN;
  abi: ContractInterface;
  bytecode: BytesLike;
}

/** Contract factories. */
export class ContractFactory implements ProjectFactory {
  readonly artifact: Artifact;
  readonly ethersFactory: ethers.ContractFactory;
  saveReceipt?: SaveReceiptFunction;

  /** Create new contract factory.
   * @param {Artifact} art - The contract artifact.
   * @param {Options?} options - Optional arguments.
   */
  constructor(art: Artifact, options?: ContractFactoryOptions) {
    this.artifact = art;
    this.saveReceipt = options?.saveReceipt;
    this.ethersFactory = new ethers.ContractFactory(art.abi, art.bytecode, options?.signer);
  }

  /** @return {ContractFQN} fully qualified contract name */
  fqn(): ContractFQN {
    return this.artifact.fqn;
  }

  /** Deploy contract and save receipts to disk.
   * @param {Array<any>} args - The constructor arguments.
   * @return {Promise<Contract>} The deployed contract.
   * @throws {Error} If the contract could not be deployed.
   */
  async deploy(...args: Array<any>): // eslint-disable-line @typescript-eslint/no-explicit-any
    Promise<Contract> {
    // deploy contract
    const contract = await this.ethersFactory.deploy(...args);
    // wait for receipt
    const receipt = await contract.deployTransaction.wait();
    // save receipt
    if (this.saveReceipt) {
      await this.saveReceipt(receipt);
    }
    return contract;
  }
}

/** The project handler for Solidity projects. */
export class Handler extends TargetProjectHandler {
  private net_cfg: EndpointConfig;
  private provider: ethers.providers.JsonRpcProvider;

  /** Create new project handler.
   * @param {TargetProject} project - The project to handle.
   */
  constructor(project: TargetProject) {
    super(project);
    this.net_cfg = project.config.network_for_target(project.target());
    if (!this.net_cfg) {
      throw new Error(`Missing network configuration for target '${project.target()}'`);
    }
    const url = this.net_cfg.proxy ?
      `http://localhost:${this.net_cfg.proxy.port}` :
      this.net_cfg.url.toString();
    this.provider = new ethers.providers.JsonRpcProvider(url);
  }

  /** Get ethers ContractFactory for the given contract name. We assume that
   * there is only one contract with the given name for each project (target).
   * @param {ContractName} name - The contract name.
   * @return {ContractFactory} The contract factory.
   * @throws {Error} If the contract factory cannot be found.
   * */
  getContractFactory(name: ContractName): ContractFactory {
    const artifact = this.getBuildArtifact(name);
    const signer = this.getSigner();
    const saveReceipt = async (receipt: ContractReceipt) => {
      await this.saveDeployReceipt(name, receipt);
    };
    return new ContractFactory(artifact, { signer, saveReceipt, });
  }

  /** Save the deployment receipt to disk.
   * We save receipts in:
   * <deploy_dir>/<target>/<current_network_profile>/<contract_name>/<contract_address>.json
   * @param {ContractName} name - The contract name.
   * @param {ContractReceipt} receipt - The contract receipt.
   */
  private async saveDeployReceipt(name: ContractName, receipt: ContractReceipt): Promise<void> {
    const proj = this.project;
    // Create deploy/current-network-profile/target/contract-name
    const deploy_dir = path.join(proj.config.deploy_dir(), proj.target(),
      proj.config.current_network_profile, name);
    await mkdir(deploy_dir, { recursive: true, });
    // The file name is the contract address (since we might deploy multiple
    // contracts with the same name)
    const file_name = `${receipt.contractAddress}.json`;
    // Write receipt to file
    await writeFile(path.join(deploy_dir, file_name), JSON.stringify(receipt, null, 2));
  }

  /** Get deployed contract.
  * @param {ContractName} name - The contract name.
  * @param {Address?} addr - Optional contract address (if more than one
  * contract with same name).
  * @param {boolean} ignoreReceipt - Ignore receipt (e.g., if contract deployed
  * with another tool).
  * @return {NamedContract} The contract and its name.
  * @throws {Error} If the contract could not be found, if there are multiple
  * contracts and the address argument is omitted, or if the receipt is missing
  * (unless ignoreReceipt is set).
  * */
  getNamedContract(name: ContractName, addr?: Address, ignoreReceipt = false): NamedContract {
    // Get the artifact (throws if we didn't build the contract)
    const artifact = this.getBuildArtifact(name);
    // Find the contract address in the deploy directory
    const proj = this.project;
    // Get the contract deploy dir
    const deploy_dir = path.join(proj.config.deploy_dir(), proj.target(),
      proj.config.current_network_profile, name);
    if (!addr) {
      // Get deploy receipt(s)
      const files = fs.existsSync(deploy_dir) ? fs.readdirSync(deploy_dir) : [];

      if (files.length === 0) {
        throw new Error(`Could not find deploy receipts for contract '${name}' on '${proj.target()}'.`);
      }
      if (files.length > 1) {
        throw new Error(`More than one contract '${name}' found on '${proj.target()}' in '${deploy_dir}': ${files}`);
      }
      // get address by reading the receipt (even though for now the file name has the address)
      const receipt: ContractReceipt =
        JSON.parse(fs.readFileSync(path.join(deploy_dir, files[0]), 'utf8'));
      addr = receipt.contractAddress;
    } else {
      if (!ethers.utils.isAddress(addr)) {
        throw new Error(`Invalid contract address '${addr}'`);
      }
      if (!ignoreReceipt) {
        // get receipt (assuming contract was deployed using our tooling)
        const receipt_file = path.join(deploy_dir, `${addr}.json`);
        if (!fs.existsSync(receipt_file)) {
          throw new Error(`Could not find deploy receipt for contract '${name}' on '${proj.target()}' at address '${addr}'.`);
        }
        // Read receipt and double check address
        const receipt: ContractReceipt = JSON.parse(fs.readFileSync(receipt_file, 'utf8'));
        if (addr !== receipt.contractAddress) {
          throw new Error(`Unexpected: Bad receipt for contract '${name}' on '${proj.target()}' at address '${addr}'. Please report this bug.`);
        }
      }
    }

    return <NamedContract>{
      fqn: artifact.fqn,
      inner: new ethers.Contract(addr, artifact.abi, this.getSigner()),
    };
  }

  /** Check if contract has been deployed.
  * @param {ContractName} name - The contract name.
  * @return {boolean} true if contract was deployed at least once.
  * */
  isDeployed(name: ContractName): boolean {
    // Find the contract address in the deploy directory
    const proj = this.project;
    // Get the contract deploy dir
    const deploy_dir = path.join(proj.config.deploy_dir(), proj.target(),
      proj.config.current_network_profile, name);
    // Get deploy receipt(s)
    const files = fs.existsSync(deploy_dir) ? fs.readdirSync(deploy_dir) : [];
    return (files.length > 0);
  }

  /** Get build artifact for the given contract name.
   * TODO: make sure the JSON is valid.
   * @param {ContractName} name - The contract name.
   * @return {JSON} The build artifact.
   */
  private getBuildArtifact(name: ContractName): Artifact {
    const proj = this.project;
    // make sure the artifacts are built
    const artifacts_dir = path.join(proj.config.build_dir(), proj.target(), 'artifacts');
    if (!fs.existsSync(artifacts_dir)) {
      throw new Error(`Artifacts directory '${artifacts_dir}' does not exist. Did you run 'cubist build'?`);
    }
    // find the contract (${name}.json) in one of the *.sol directories
    const artifacts = fs.readdirSync(artifacts_dir);
    for (const sol_dir of artifacts) {
      const json_files = fs.readdirSync(path.join(artifacts_dir, sol_dir));
      const contract_file = `${name}.json`;
      if (json_files.indexOf(contract_file) >= 0) {
        const contract_file_path = path.join(artifacts_dir, sol_dir, contract_file);
        const fqn: ContractFQN = { file: sol_dir, name: name, };
        const json = JSON.parse(fs.readFileSync(contract_file_path, 'utf8'));
        // TODO validate the json
        return <Artifact> {
          fqn: fqn,
          abi: json.abi,
          bytecode: json.bytecode,
        };
      }
    }
    throw new Error(`Could not find artifact for ${name} on ${proj.target()}.`);
  }

  /** Get signer for the current network profile.
   * If not address or account index is provided, we use first account.
   * We'll want to change this to use different accounts for deployment, etc.
   * @param {string | number} addressOrIndex - Optional account index or address.
   * @return {Signer} The signer.
   */
  private getSigner(addressOrIndex?: Address | number): Signer {
    const account = addressOrIndex ?? /* account @ index */ 0;
    return this.provider.getSigner(account);
  }

  /** Retrieve all accounts used on this target.
   * @return {Promise<Address[]>} Accounts.
   */
  accounts(): Promise<Address[]> {
    return this.provider.listAccounts();
  }

  /** Return default signer address.
   * @return {Promise<Address>} Default signer address. */
  getSignerAddress(): Promise<Address> {
    return this.getSigner().getAddress();
  }

  /** Get the balance of the given address.
    * @param {Address} addr - The address.
    * @return {Promise<BigNumber>} The balance. */
  getBalance(addr: Address): Promise<BigNumber> {
    return this.provider.getBalance(addr);
  }
}
