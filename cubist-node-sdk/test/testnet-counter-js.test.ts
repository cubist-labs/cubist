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
import { warn, } from 'console';

// build, deploy, etc take a while...
jest.setTimeout(60000);

dotenvNearest(__dirname);

const TESTNET_MNEMONIC_ENV_VAR_NAME = 'CUBIST_TESTNET_MNEMONIC';

// only run this if we have a testnet mnemonic
const SKIP = !(TESTNET_MNEMONIC_ENV_VAR_NAME in process.env);
const describeOrSkip = SKIP ? describe.skip : describe;

// Global config we share across tests.
let testdk: CubistTestDK;

beforeAll(async () => {
  if (!SKIP) {
    testdk = new CubistTestDK({
      tmp_build_dir: true,
      tmp_deploy_dir: true,
      args: [
        Config.from_file(path.join(__dirname, 'fixtures',
          'project-fixtures', 'counter', 'cubist-config.json'))
      ],
    });
    setCubistBinToCargoBuildBin();
    await testdk.build();
    await testdk.startService();
    testdk.stopServiceOnExit();
    info('Started chains and relayer');
  } else {
    warn(`*** WARN ***: Set '${TESTNET_MNEMONIC_ENV_VAR_NAME}' env var to run this test`);
  }
});

afterAll(async () => {
  if (!SKIP) {
    await testdk.stopService();
  }
});

describeOrSkip('Cubist', () => {
  describe('deploy and test', () => {
    it('works', async () => {
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
      expect(await (await fromCnt.inner.store(33)).wait(/* confirmations: */ 1)).to.not.throw;

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
    });
  });
});
