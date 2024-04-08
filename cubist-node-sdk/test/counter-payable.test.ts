import { expect, } from 'chai';
import {
  Config,
  Contract,
  EthersContract,
  CubistTestDK,
} from '../';
import {
  dotenvNearest,
  info,
  setCubistBinToCargoBuildBin,
} from './utils';
import * as path from 'path';

// build, deploy, etc take a while...
jest.setTimeout(60000);

dotenvNearest(__dirname);

// Global config we share across tests.
let testdk: CubistTestDK;

afterEach(async () => {
  await testdk.stopService();
});

describe('Cubist', () => {
  describe('eth_poly', () => {
    beforeEach(async () => {
      await setup([
        ['__FROM_TARGET__', 'ethereum'],
        ['__TO_TARGET__', 'polygon']
      ]);
    });
    it('works', doTest);
  });
  describe('poly_eth', () => {
    beforeEach(async () => {
      await setup([
        ['__FROM_TARGET__', 'polygon'],
        ['__TO_TARGET__', 'ethereum']
      ]);
    });
    it('works', doTest);
  });
});

/**
 * Set up the cubist project.
 * @param {[string, string][]} remapping - Rewrite config file before relocating to temp dir
 */
async function setup(remapping: [string, string][]) {
  const cfgPath = CubistTestDK.copyToTmpDir(
    path.join(__dirname, 'fixtures', 'project-fixtures', 'counter_payable', 'cubist-config.json'),
    remapping);
  testdk = new CubistTestDK({
    args: [Config.from_file(cfgPath)],
  });
  setCubistBinToCargoBuildBin();
  await testdk.build();
  await testdk.startService();
  testdk.stopServiceOnExit();
  info('Started chains and relayer');
}

/** Do the test */
async function doTest() {
  const cubist = testdk.cubist;
  const FromCounter = cubist.getContractFactory('From');
  const ToCounter = cubist.getContractFactory('To');

  // deploy 'to' counter
  const toCnt: Contract<EthersContract> = await ToCounter.deploy();
  expect(toCnt).is.not.null;

  // deploy 'from' counter
  const fromCnt = await FromCounter.deploy(toCnt.addressOn(FromCounter.target()));
  expect(fromCnt).is.not.null;

  // wait for the bridge to be established before calling any cross-chain contract methods
  expect(await cubist.whenBridged()).is.true;

  // set value
  expect(await (await fromCnt.inner.store(33, { value: BigInt(5_000_000), }))
    .wait(/* confirmations: */ 1)).to.not.throw;

  // check value
  expect((await fromCnt.inner.retrieve()).eq(33)).is.true;

  // see if the value gets propagated to ToCounter
  let toVal = 0;
  for (let i = 0; i < 50; i++) {
    toVal = await toCnt.inner.retrieve();
    if (toVal == 33) {
      toVal = 33;
      break;
    }
    await new Promise((r) => setTimeout(r, 200));
  }
  expect(toVal).eq(33);
}
