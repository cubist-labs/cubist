import { expect, } from 'chai';
import {
  Target,
  internal,
  ConfigTestDK,
} from '../';
import {
  info,
  verbose,
  setCubistBinToCargoBuildBin,
} from './utils';
import * as path from 'path';
import { ethers, } from 'ethers';

// build, deploy, etc take a while...
jest.setTimeout(60000);

export const TargetProject = internal.TargetProject;

// Global config we share across tests.
const testdk = new ConfigTestDK(path.join(__dirname, 'fixtures',
  'poly-eth-counter-js', 'cubist-config.json'), {
  tmp_build_dir: true,
  tmp_deploy_dir: true,
});
const g_cfg = testdk.config;

beforeAll(async () => {
  setCubistBinToCargoBuildBin();
  await testdk.build();
  await testdk.startService();
  testdk.stopServiceOnExit();
  info('Started chains and relayer');
});

afterAll(async () => {
  await testdk.stopService();
});

describe('TargetProject', () => {
  describe('create project', () => {
    it('creates cubist projects', () => {
      verbose('it(\'creates cubist project\')');
      const polygon = new TargetProject(Target.Polygon, g_cfg);
      const PolyCounter = polygon.getContractFactory('PolyCounter');
      expect(PolyCounter).is.not.null;

      const ethereum = new TargetProject(Target.Ethereum, g_cfg);
      const EthCounter = ethereum.getContractFactory('EthCounter');
      expect(EthCounter).is.not.null;
    });

    it('fails for real cubist project but fake contract', () => {
      verbose('it(fails for real)');
      const polygon = new TargetProject(Target.Polygon, g_cfg);
      expect(() => polygon.getContractFactory('FakeCounter')).to.throw();
      expect(() => polygon.getContract('FakeCounter')).to.throw();
    });
  });

  describe('deploy contracts', () => {
    describe('once', () => {
      it('deploys contract on polygon', async () => {
        verbose('it(deploys contract)');
        const polygon = new TargetProject(Target.Polygon, g_cfg);
        const PolyCounter = polygon.getContractFactory('PolyCounter');

        // deploy eth counter shim
        const EthCounter = polygon.getContractFactory('EthCounter');
        const ethCounter0 = await EthCounter.deploy();
        expect(ethCounter0).is.not.null;
        expect(ethCounter0).is.instanceOf(ethers.Contract);
        expect(path.join(g_cfg.deploy_dir(), 'polygon', g_cfg.current_network_profile, 'ethCounter', `${ethCounter0.address}.json`)).to.exist;

        // deploy poly counter
        const polyCounter0 = await PolyCounter.deploy(55, ethCounter0.address);
        expect(polyCounter0).is.not.null;
        expect(polyCounter0).is.instanceOf(ethers.Contract);
        expect(path.join(g_cfg.deploy_dir(), 'polygon', g_cfg.current_network_profile, 'PolyCounter', `${polyCounter0.address}.json`)).to.exist;

        // get contract
        const polyCounter1 = polygon.getContract('PolyCounter');
        expect(polyCounter1).is.not.null;
        expect(polyCounter1).is.instanceOf(ethers.Contract);
        expect(polyCounter1.address).is.equal(polyCounter0.address);

        // check value
        expect((await polyCounter1.retrieve()).eq(55)).is.true;

        // set value -> throws because it shouldn't be allowed to call 'store' on ethCounter0
        const confirmations = 1;
        const expectedError = /Cubist: sender is not a caller/;
        try {
          await (await polyCounter1.store(33)).wait(confirmations);
          fail(`Expected 'store' to fail with ${expectedError}`);
        } catch (e) {
          expect(e).to.match(expectedError);
        }

        // call 'approveCaller' then try again
        expect(await ethCounter0.approveCaller(polyCounter1.address)).to.not.throw;
        expect(await (await polyCounter1.store(33)).wait(confirmations)).to.not.throw;

        // check value
        expect((await polyCounter1.retrieve()).eq(33)).is.true;

        // real contract, bad address
        expect(() => polygon.getContract('PolyCounter', '0x0f00bar')).
          to.throw(/Invalid contract address/);

        // fail to get fake contract
        expect(() => polygon.getContract('FakeCounter')).to.throw(/Could not find/);
      });

      it('deploys contract on ethereum', async () => {
        verbose('it(deploys eth)');
        const ethereum = new TargetProject(Target.Ethereum, g_cfg);
        const EthCounter = ethereum.getContractFactory('EthCounter');

        // deploy
        const ethCounter0 = await EthCounter.deploy(67);
        expect(ethCounter0).is.not.null;
        expect(ethCounter0).is.instanceOf(ethers.Contract);
        expect(path.join(g_cfg.deploy_dir(), 'ethereum', g_cfg.current_network_profile, 'EthCounter', `${ethCounter0.address}.json`)).to.exist;
        expect((await ethCounter0.retrieve()).eq(67)).is.true;

        // get contract
        const ethCounter1 = ethereum.getContract('EthCounter');
        expect(ethCounter1).is.not.null;
        expect(ethCounter1).is.instanceOf(ethers.Contract);
        expect(ethCounter1.address).is.equal(ethCounter0.address);

        // real contract, bad address
        expect(() => ethereum.getContract('EthCounter', '0x0f00bar')).
          to.throw(/Invalid contract address/);

        // fail to get fake contract
        expect(() => ethereum.getContract('FakeCounter')).to.throw(/Could not find/);
      });
    });

    describe('multiple times', () => {
      it('deploys contracts', async () => {
        const zero_address = '0x0000000000000000000000000000000000000000';
        const polygon = new TargetProject(Target.Polygon, g_cfg);
        const PolyCounter = polygon.getContractFactory('PolyCounter');

        // deploy one
        const polyCounter0 = await PolyCounter.deploy(1337, zero_address);
        expect(polyCounter0).is.not.null;
        expect(polyCounter0).is.instanceOf(ethers.Contract);
        expect(path.join(g_cfg.deploy_dir(), 'polygon', g_cfg.current_network_profile, 'PolyCounter', `${polyCounter0.address}.json`)).to.exist;
        expect((await polyCounter0.retrieve()).eq(1337)).is.true;

        // deploy two
        const polyCounter1 = await PolyCounter.deploy(1, zero_address);
        expect(polyCounter1).is.not.null;
        expect(polyCounter1).is.instanceOf(ethers.Contract);
        expect(path.join(g_cfg.deploy_dir(), 'polygon', g_cfg.current_network_profile, 'PolyCounter', `${polyCounter1.address}.json`)).to.exist;
        expect((await polyCounter1.retrieve()).eq(1)).is.true;

        // get contract without address should fail
        expect(() => polygon.getContract('PolyCounter')).to.throw(/More than one/);

        const polyCounter0a = polygon.getContract('PolyCounter', polyCounter0.address);
        expect(polyCounter0a).is.not.null;
        expect(polyCounter0a).is.instanceOf(ethers.Contract);
        expect(polyCounter0a.address).is.equal(polyCounter0.address);

        const polyCounter1a = polygon.getContract('PolyCounter', polyCounter1.address);
        expect(polyCounter1a).is.not.null;
        expect(polyCounter1a).is.instanceOf(ethers.Contract);
        expect(polyCounter1a.address).is.equal(polyCounter1.address);
      });
    });
  });
});
