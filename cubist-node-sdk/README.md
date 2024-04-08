This README is best viewed on the official [Cubist docs
site](https://docs.cubist.dev/jsdoc).

# Overview

Cubist makes it easy to develop **cross-chain** dApps by making them look and
feel like single-chain dApps.

With Cubist, you write contracts as if they were all going to be deployed on
the same chain.  This means you can have a contract directly call a method on
another contract, as if they were on the same chain. You don't need to commit
to the chain you're going to deploy a contract to in advance nor clutter your
code with low-level, error-prone message passing code. Instead, with Cubist,
you specify your deployment plan in a [config file][CubistConfig] by mapping
contract source files to target chains.

Using the [cubist cli][CubistCLI] tool you can then to compile such a dApp to
run on multiple chains. Behind the scenes, our tool automatically creates:

1. A _target project_ for each chain you specify in the configuration. A target
   project is a standard single-chain project.

2. A **shim** contract for every for every contract that is called
   cross-chain[^xchain-deps]. These shims live in the target project for the
   chain on which that contract is called. Shims facilitate the cross-chain
   interactions between contracts---they implement the message passing code.

The CLI tool also comes with local network support and an off-chain
relayer[^relayer]. In the next release we'll have support for relaying via
bridge providers like Axelar which you'll be able to use by just modifying the
configuration.

Finally, this Cubist SDK is designed to help you write applications in
TypeScript and JavaScript that interact with these contracts. (If you prefer
Rust, checkout our [Rust SDK][RustSDK].)

## Node.js SDK at a glance

The Node.js SDK exposes several high-level abstractions for working with smart
contracts:

- The [Cubist] class abstracts your cubist project and is _the_ way to access
  contracts and contract factories. Given a Cubist instance (which you can
  create with `new Cubist()`), you can use:

  - [getContractFactory] to get the [ContractFactory] corresponding to say
    the `Receiver` or `Sender` contracts above---and use this factory to
    [deploy] an instance of each contract or get an already deployed contract
    (with [deployed] or [attach]).

  - [getContract] to get an already deployed [Contract] instance of, say,
    `Receiver`. You can then use the contract instance to interact with the
    on-chain contract (e.g. call `store()` and `retrieve()` on them).

  - [whenBridged] to wait for the relayer to set up the off-chain cross-chain
    bridge (i.e., essentially wait for `cubist start relayer`).

  We describe factories and contracts in more detail in their corresponding
  pages. The short: they're thin wrappers over [ether.js][ether.js] factories
  and contracts (for now) that know about cross-chain details.

  Internally, [Cubist], contract factories, and contracts keep track of the
  different chains and the relationship between them (e.g., contract shims);
  in general, though, you should not need to use this per-chain
  [target project][TargetProject] interface.

- The [TestDK] class abstracts local testing for [Cubist] and [CubistORM]
  projects. Instead of wrapping your tests with scripts that build the project,
  start and stop services (chains and relayer), [TestDK] exposes an interface
  that lets you do this in your Node.js tests, and do so in temporary
  directories so you don't clobber build and deploy directories.

- The [CubistORM] class we generate at build time. When you run `cubist build`,
  our toolchain also generates an [ORM] for your project (in the `./build/orm`
  directory). Specifically, the ORM module exports a `CubistORM` class that
  extends the [Cubist] class to directly expose factories as properties on the
  object---e.g., `Receiver` and `Sender`. This is the case of both JavaScript
  and TypeScript project---but TypeScript get the extra win of type-safety: we
  use [TypeChain] to expose well-type contracts.


[Install](/guide/Installation) Cubist if you haven't already and let's try this
SDK in action!

## Example: Cross-chain storage dApp

To start, let's say we want to build a simple cross-chain app that just stores
a value (`uint256`) across two contracts. First, let's create an empty
TypeScript project:

```bash
cubist new --type TypeScript dApp
cd dApp
yarn # or npm i
```

If you'd rather use JavaScript over TypeScript, you can---just ignore any type
annotations in our example (and use .js vs .ts files and `node` instead of
`ts-node`).

Then let's create two solidity files in the `contracts` directory:

- `Receiver.sol`, which simply exposes a simple contract for storing a number:

    ```solidity
    // SPDX-License-Identifier: UNLICENSED
    pragma solidity ^0.8.16;

    contract Receiver {
      uint256 _number;

      function store(uint256 num) public {
        _number = num;
      }

      function retrieve() public view returns (uint256) {
        return _number;
      }
    }
    ```

- `Sender.sol` exposes almost the same contract. It differs in only one
  way---it stores the number locally and on the `Receiver` contract
  (whose address we supply when we deploy `Sender`):

    ```solidity
    // SPDX-License-Identifier: UNLICENSED
    pragma solidity ^0.8.16;

    import './Receiver.sol';

    contract Sender {
      Receiver _receiver;
      uint256 _number;

      constructor (Receiver addr) {
        _receiver = addr;
      }

      function store(uint256 num) public {
        _number = num;
        _receiver.store(_number);
      }

      function retrieve() public view returns (uint256) {
        return _number;
      }
    }
    ```

Let's also say we want deploy `Receiver` to Ethereum and the `Sender` to
Polygon. To do this, we to need update the `contracts` build and deployment
plan in `cubst-config.json`:

```json
{
...
  "contracts": {
    "root_dir": "contracts",
    "targets": {
      "ethereum" : {
        "files": ["./contracts/Receiver.sol"]
      },
      "polygon": {
        "files": ["./contracts/Sender.sol"]
      }
    },
  },
...
}
```

This tells Cubist where contract files are (in the `contracts` directory), and
what target chain any particular contract file should be compiled for. We
describe configurations in more detail (e.g., how to configure networks)
elsewhere[^ref-config].

We can build this dApp with `cubist build`. The build command generates two
target projects (in the `build` directory), one for each chain:

- the `ethereum` project contains only the `Receiver` contract (unchanged),

- the `polygon` project contains the `Sender` contract (unchanged) as well as
  an automatically generated `Receiver` shim contract; the shim contract has
  exactly the same interface as the original receiver contract (so that
  `Sender` can remain unchanged).  The key difference, however, is that the
  shim contract's `store` method now only generates an event (containing the
  method argument in its field).  This event is automatically picked up by the
  relayer and relayed to the original `Receiver` contract deployed on Ethereum.

After creating the shims in each target project, `cubist build` also builds
each target project individually using a native contract compiler (currently,
`solc` is the only supported compiler for contracts written in [Solidity]).

We're now ready to deploy, test, and interact with these contract. Cubist has
two SDKs: a [Rust SDK][RustSDK] and a Node.js (TypeScript and JavaScript) SDK.
We describe the core abstractions exposed the Node.js SDK here.

### Deploy `Sender` and `Receiver` contracts then call `store`

#### Using Cubist

Let's create a script `deployAndStoreCubist.ts` in the `src` directory, and use
the [Cubist] interface to deploy both contracts, call `store()` on the `Sender`,
and ensure sure the value is `retrieve`d on both chains:

```typescript
import { Target, Cubist, } from '@cubist-labs/cubist'
import assert from 'assert';

async function main() {
  // create Cubist instance (looks for 'cubist-config.json' in parent directories)
  const cubist = new Cubist();

  // get contract factories
  const Receiver = cubist.getContractFactory('Receiver');
  const Sender = cubist.getContractFactory('Sender');

  // deploy Receiver
  // - this deploys the Receiver contract on Ethereum
  // - and a shim Receiver contract on Polygon
  const receiver = await Receiver.deploy();

  // - this constructor takes the address of the Receiver on Sender's target
  //   chain (Polygon)
  const sender = await Sender.deploy(receiver.addressOn(Target.Polygon));

  // wait for relayer to start bridging
  assert(await cubist.whenBridged());

  // call store on the Sender
  // - we do this by getting the inner ethers.js Contract and just invoking
  //   methods on it as usual; see https://docs.ethers.org/v5/api/contract/contract/
  const txResponse = await sender.inner.store(123);
  //  - wait for at least 1 confirmation that the store happened
  await txResponse.wait();

  // if the relayer is running, it will automatically propagate the value to
  // the Receiver; since this might take a bit lets sleep for 1 second
  await sleep(1000);

  // call retrieve on both Sender and Receiver
  assert((await sender.inner.retrieve()).eq(123));
  assert((await receiver.inner.retrieve()).eq(123));
}
main().catch(console.error);

async function sleep(ms: number) {
  return new Promise(resolve => setTimeout(resolve, ms));
}
```

To run this script we first need to run chains locally. To do this, we need to
first extend our `cubist-config.json`:

```json
...
  "network_profiles": {
    "default": {
      "ethereum": { "url": "http://127.0.0.1:8545/" },
      "polygon":  { "url": "http://127.0.0.1:9545" }
    }
  },
...
}
```

Now we can run `cubist start`, which will start (1) a local Ethereum and
Polygon node at the URLs defined in the config file and (2) a relayer between
the two chains.

To run the script:

```bash
./node_modules/.bin/ts-node-esm ./src/deployAndStoreCubist.ts
```

This will will deploy the contracts and save all deployment info in the
`deploy` directory. For now, you can only deploy a single instance of a
contract (this will change in the next release), so if you want to run this
script again you'll need to blow away the `deploy` directory.

#### Using CubistORM

The above is a bit verbose and, unfortunately, when we interact with the
underlying [ethers.js] contract to e.g., call `store` and `retrieve`, this is
largely using dynamically typed---[getContractFactory] returns a `Contract` on
a particular chain (it doesn't know the type of `Sender` or `Receiver`
specifically). This is where [CubistORM] comes into play and why generate code at
build time. So, let's use this to write a well-typed script (in this case
`src/index.ts`) using the ORM:

```typescript
import {
  CubistORM,
  Receiver,
  Sender,
  Polygon,
} from '../build/orm/index.js';
import assert from 'assert';

async function main() {
  // create CubistORM instance (looks for 'cubist-config.json' in parent directories)
  const cubist = new CubistORM();

  // deploy Receiver
  // - this deploys the Receiver contract on Ethereum
  // - and a shim Receiver contract on Polygon
  const receiver:Receiver = await cubist.Receiver.deploy();

  // deploy Sender
  // - this constructor takes the address of the Receiver on Sender's target
  //   chain (Polygon)
  const sender:Sender = await cubist.Sender.deploy(receiver.addressOn(Polygon));

  // wait for relayer to start bridging
  assert(await cubist.whenBridged());

  // call store on the Sender
  // - we do this by getting the inner ethers.js Contract and just invoking
  //   methods on it as usual; see https://docs.ethers.org/v5/api/contract/contract/
  // - Using CubistORM instead of Cubist, though, ensures that all interactions are
  //   well-typed (we use type chain to specify the type of the `inner` Contract)
  const txResponse = await sender.inner.store(123);
  //  - wait for at least 1 confirmation that the store happened
  await txResponse.wait();

  // if the relayer is running, it will automatically propagate the value to
  // the Receiver; since this might take a bit lets sleep for 1 second
  await sleep(1000);

  // call retrieve on both Sender and Receiver
  assert((await sender.inner.retrieve()).eq(123));
  assert((await receiver.inner.retrieve()).eq(123));
}
main().catch(console.error);

async function sleep(ms: number) {
  return new Promise(resolve => setTimeout(resolve, ms));
}
```

You can run this as we did with `deployAndStoreCubist.ts` above.

### Load already deployed contracts from existing deployment receipts

Deploying contracts saves receipts in the `deploy` directory as mentioned
above. Unlike other frameworks, which modify the compiled artifacts (the ABI
files produced during compilation), Cubist separates build and deployment
artifacts. For now, the deployment receipts are stored on the filesystem; in
production, and in future releases, we'll have support for writing these
receipts to other persistent storage (namely databases).

We can interact with already deployed contracts as you might expect:

#### Using Cubist

```typescript
import { Cubist, } from '@cubist-labs/cubist'
import assert from 'assert';

async function main() {
  // create Cubist instance (looks for 'cubist-config.json' in parent directories)
  const cubist = new Cubist();

  // Get deployed contracts
  const receiver= cubist.getContract('Receiver');
  const sender = cubist.getContract('Sender');

  // wait for relayer to start bridging
  assert(await cubist.whenBridged());

  // call store on the Sender
  // - we do this by getting the inner ethers.js Contract and just invoking
  //   methods on it as usual; see https://docs.ethers.org/v5/api/contract/contract/
  // - Using CubistORM instead of Cubist, though, ensures that all interactions are
  //   well-typed (we use type chain to specify the type of the `inner` Contract)
  const txResponse = await sender.inner.store(456);
  //  - wait for at least 1 confirmation that the store happened
  await txResponse.wait();

  // if the relayer is running, it will automatically propagate the value to
  // the Receiver; since this might take a bit lets sleep for 1 second
  await sleep(1000);

  // call retrieve on both Sender and Receiver
  assert((await sender.inner.retrieve()).eq(456));
  assert((await receiver.inner.retrieve()).eq(456));
}
main().catch(console.error);

async function sleep(ms: number) {
  return new Promise(resolve => setTimeout(resolve, ms));
}

```

#### Using CubistORM

```typescript
import {
  CubistORM,
  Receiver,
  Sender,
} from '../build/orm/index.js';
import assert from 'assert';

async function main() {
  // create CubistORM instance (looks for 'cubist-config.json' in parent directories)
  const cubist = new CubistORM();

  // Get deployed contracts
  const receiver:Receiver = await cubist.Receiver.deployed();
  const sender:Sender = await cubist.Sender.deployed();

  // wait for relayer to start bridging
  assert(await cubist.whenBridged());

  // call store on the Sender
  // - we do this by getting the inner ethers.js Contract and just invoking
  //   methods on it as usual; see https://docs.ethers.org/v5/api/contract/contract/
  // - Using CubistORM instead of Cubist, though, ensures that all interactions are
  //   well-typed (we use type chain to specify the type of the `inner` Contract)
  const txResponse = await sender.inner.store(456);
  //  - wait for at least 1 confirmation that the store happened
  await txResponse.wait();

  // if the relayer is running, it will automatically propagate the value to
  // the Receiver; since this might take a bit lets sleep for 1 second
  await sleep(1000);

  // call retrieve on both Sender and Receiver
  assert((await sender.inner.retrieve()).eq(456));
  assert((await receiver.inner.retrieve()).eq(456));
}
main().catch(console.error);

async function sleep(ms: number) {
  return new Promise(resolve => setTimeout(resolve, ms));
}
```

[Cubist]: /jsdoc/classes/cubist.Cubist
[getContractFactory]: /jsdoc/classes/cubist.Cubist#getcontractfactory
[getContract]: /jsdoc/classes/cubist.Cubist#getcontract
[whenBridged]: /jsdoc/classes/cubist.Cubist#whenbridged
[CubistORM]: /jsdoc-md/cubist.CubistORM

[TargetProject]: /jsdoc-md/cubist.internal.TargetProject

[ContractFactory]: /jsdoc/classes/cubist.ContractFactory
[deploy]: /jsdoc/classes/cubist.ContractFactory#deploy
[deployed]: /jsdoc/classes/cubist.ContractFactory#deployed
[attach]: /jsdoc/classes/cubist.ContractFactory#attach

[Contract]: /jsdoc/classes/cubist.Contract

[TestDK]: /jsdoc/classes/test.TestDK

[CubistCLI]: /guide/cli
[CubistConfig]: /guide/config
[RustSDK]: pathname:///rustdoc/cubist_sdk/

[ORM]: https://en.wikipedia.org/wiki/Object%E2%80%93relational_mapping
[TypeChain]: https://github.com/dethcrypto/TypeChain
[ether.js]: https://ethers.org/
[Solidity]: https://soliditylang.org/

[^xchain-deps]: Cubist automatically discovers all cross-chain dependencies by
  statically analyzing the contract source files.

[^relayer]: The relayer, which you can start with `cubist start relayer`
  continuously monitors events triggered by shim contracts and automatically
  relays them to their final destinations.

[^ref-config]: Refer to [CubistConfig](config file reference) for details on
  how to configure a Cubist dApp and assign contracts to different chains.
