import { expect, } from 'chai';
import {
  Config,
  Target,
  Contract, ContractFactory,
  EthersContract,
  CubistTestDK,
} from '../';
import {
  info,
  setCubistBinToCargoBuildBin,
} from './utils';
import * as path from 'path';
import { ethers, } from 'ethers';

// build, deploy, etc take a while...
jest.setTimeout(60000);

// Global config we share across tests.
const testdk = new CubistTestDK({
  tmp_build_dir: true,
  tmp_deploy_dir: true,
  args: [
    Config.from_file(path.join(__dirname, 'fixtures', 'poly-eth-counter-js', 'cubist-config.json'))
  ],
});

beforeAll(async () => {
  setCubistBinToCargoBuildBin();
  await testdk.build();
  await testdk.startService();
  testdk.stopServiceOnExit();
  info('Started chains and relayer');
});

beforeEach(async () => {
  await testdk.emptyDeployDir();
});

afterAll(async () => {
  await testdk.stopService();
});

describe('Cubist', () => {
  describe('getContractFactory', () => {
    it('gets factory for real contracts', () => {
      const cubist = testdk.cubist;
      const PolyCounter: ContractFactory<EthersContract> = cubist.getContractFactory('PolyCounter');
      expect(PolyCounter).is.not.null;

      const EthCounter: ContractFactory<EthersContract> = cubist.getContractFactory('EthCounter');
      expect(EthCounter).is.not.null;
    });

    it('fails for fake contract', () => {
      const cubist = testdk.cubist;
      expect(() => cubist.getContractFactory('FakeCounter')).to.throw();
      expect(() => cubist.getContract('FakeCounter')).to.throw();
    });
  });

  describe('deploy contracts', () => {
    describe('once', () => {
      it('deploys contract on polygon', async () => {
        const cubist = testdk.cubist;
        const cfg = cubist.config;

        const PolyCounter = cubist.getContractFactory('PolyCounter');
        const EthCounter = cubist.getContractFactory('EthCounter');

        // deploy eth counter shim
        const ethCnt0: Contract<EthersContract> = await EthCounter.deploy(0);
        expect(ethCnt0).is.not.null;
        expect(ethCnt0.inner).is.instanceOf(ethers.Contract);
        expect(path.join(cfg.deploy_dir(), 'polygon', cfg.current_network_profile, 'ethCounter',
          `${ethCnt0.addressOn(Target.Polygon)}.json`)).to.exist;
        expect(path.join(cfg.deploy_dir(), 'ethereum', cfg.current_network_profile, 'ethCounter',
          `${ethCnt0.address()}.json`)).to.exist;
        expect(path.join(cfg.deploy_dir(), 'ethereum', cfg.current_network_profile, 'ethCounter',
          `${ethCnt0.addressOn(Target.Ethereum)}.json`)).to.exist;

        // deploy poly counter
        const polyCnt0 = await PolyCounter.deploy(55, ethCnt0.addressOn(PolyCounter.target()));
        expect(polyCnt0).is.not.null;
        expect(polyCnt0.inner).is.instanceOf(ethers.Contract);
        expect(path.join(cfg.deploy_dir(), 'polygon', cfg.current_network_profile, 'PolyCounter',
          `${polyCnt0.address()}.json`)).to.exist;

        // get contract
        const polyCnt1 = cubist.getContract('PolyCounter');
        expect(polyCnt1).is.not.null;
        expect(polyCnt1.inner).is.instanceOf(ethers.Contract);
        expect(polyCnt1.address()).is.equal(polyCnt0.address());

        // wait for the bridge to be established before calling any cross-chain contract methods
        expect(await cubist.whenBridged()).is.true;

        // check value
        expect((await polyCnt1.inner.retrieve()).eq(55)).is.true;

        // set value
        expect(await (await polyCnt1.inner.store(33)).wait(/* confirmations: */ 1)).to.
          not.throw;

        // check value
        expect((await polyCnt1.inner.retrieve()).eq(33)).is.true;

        // real contract, bad address
        expect(() => cubist.getContract('PolyCounter', '0x0f00bar')).
          to.throw(/Invalid contract address/);

        // fail to get fake contract
        expect(() => cubist.getContract('FakeCounter')).to.throw(/Could not find/);

        // see if the value gets propagated to ethCounter
        let ethVal = 0;
        for (let i = 0; i < 50; i++) {
          ethVal = await ethCnt0.inner.retrieve();
          if (ethVal == 33) {
            ethVal = 33;
            break;
          }
          await new Promise((r) => setTimeout(r, 200));
        }
        expect(ethVal).eq(33);
      });

      it('deploys contract on ethereum', async () => {
        const cubist = testdk.cubist;
        const cfg = cubist.config;

        const EthCounter = cubist.getContractFactory('EthCounter');

        // deploy
        const ethCnt0 = await EthCounter.deploy(67);
        expect(ethCnt0).is.not.null;
        expect(ethCnt0.inner).is.instanceOf(ethers.Contract);
        expect(path.join(cfg.deploy_dir(), 'ethereum', cfg.current_network_profile, 'EthCounter',
          `${ethCnt0.address()}.json`)).to.exist;
        expect((await ethCnt0.inner.retrieve()).eq(67)).is.true;

        // get contract
        const ethCnt1 = cubist.getContract('EthCounter');
        expect(ethCnt1).is.not.null;
        expect(ethCnt1.inner).is.instanceOf(ethers.Contract);
        expect(ethCnt1.address()).is.equal(ethCnt0.address());

        // real contract, bad address
        expect(() => cubist.getContract('EthCounter', '0x0f00bar')).
          to.throw(/Invalid contract address/);

        // fail to get fake contract
        expect(() => cubist.getContract('FakeCounter')).to.throw(/Could not find/);
      });
    });

    describe('multiple times', () => {
      it('fails because we deployed the shim already', async () => {
        const cubist = testdk.cubist;
        const cfg = cubist.config;

        const EthCounter = cubist.getContractFactory('EthCounter');
        // deploy ethcounter
        const ethCnt0 = await EthCounter.deploy(67);
        expect(ethCnt0).is.not.null;
        expect(ethCnt0.inner).is.instanceOf(ethers.Contract);
        expect(path.join(cfg.deploy_dir(), 'ethereum', cfg.current_network_profile, 'EthCounter',
          `${ethCnt0.address()}.json`)).to.exist;
        expect((await ethCnt0.inner.retrieve()).eq(67)).is.true;

        const PolyCounter = cubist.getContractFactory('PolyCounter');

        // deploy one
        const polyCnt0 = await PolyCounter.deploy(1337, ethCnt0.addressOn(PolyCounter.target()));
        expect(polyCnt0).is.not.null;
        expect(polyCnt0.inner).is.instanceOf(ethers.Contract);
        expect(path.join(cfg.deploy_dir(), 'polygon', cfg.current_network_profile, 'PolyCounter',
          `${polyCnt0.address()}.json`)).to.exist;
        expect((await polyCnt0.inner.retrieve()).eq(1337)).is.true;

        // deployng again should fail
        try {
          // using try-catch; expect(() => ).to.throw doesn't seem to work
          await PolyCounter.deploy(0, ethCnt0.addressOn(PolyCounter.target()));
          throw new Error('Expected deploy to fail');
        } catch (e) {
          expect(e.message).to
            .eq('PolyCounter was already deployed. Multiple deployments coming soon.');
        }
      });
    });
  });
});
