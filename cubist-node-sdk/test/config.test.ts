import { expect, } from 'chai';
import {
  Compiler,
  Config,
  FileNotFound,
  IPreCompileManifest,
  InvalidContractFilePaths,
  MalformedConfig,
  MissingNetworkProfile,
  PreCompileManifest,
  ProjType,
  Target,
  _testExports,
  MnemonicConfig,
  EnvVarSecret,
  FileSecret,
  KeystoreConfig,
  PrivateKeyConfig,
} from '../src/config';
import * as path from 'path';
import * as fs from 'fs';
import { chdir, cwd, } from 'process';
import { URL, } from 'url';

describe('find_file', () => {
  it('should find package.json file', () => {
    expect(_testExports.find_file('package.json', cwd())).to
      .equal(path.join(__dirname, '..', 'package.json'));
  });
  it('should find package.json file when starting in parent dir', () => {
    expect(_testExports.find_file('package.json', path.join(__dirname, '..'))).to.equal(
      path.join(__dirname, '..', 'package.json')
    );
  });
  it('should not find fakefile.biz file', () => {
    expect(() => _testExports.find_file('fakefile.biz', cwd())).to.throw(FileNotFound);
  });
});

describe('Config.from_file', () => {
  describe('good config', () => {
    it('create config', () => {
      const dir = path.join(__dirname, 'fixtures', 'config-fixtures');
      const cfg = Config.from_file(
        path.join(__dirname, 'fixtures', 'config-fixtures', 'good-config.json')
      );
      expect(cfg.type).to.equal(ProjType.JavaScript);
      expect(cfg.build_dir()).to.equal(path.normalize(path.join(dir, 'build_dir')));
      expect(cfg.deploy_dir()).to.equal(path.normalize(path.join(dir, '..', 'deploy_dir')));
      const contracts = cfg.contracts();
      expect(contracts.root_dir).to.equal(path.normalize(path.join(dir, 'contracts')));
      expect(Object.fromEntries(contracts.targets)).to.deep.equal({
        avalanche: {
          files: [path.normalize(path.join(dir, 'contracts', 'ava.sol'))],
          compiler: Compiler.Solc,
        },
        polygon: {
          files: [path.normalize(path.join(dir, 'contracts', 'poly.sol'))],
          compiler: Compiler.Solc,
        },
        ethereum: {
          files: [path.normalize(path.join(dir, 'contracts', 'eth.sol'))],
          compiler: Compiler.Solc,
        },
      });
      // test relative_to_root
      expect(contracts.relative_to_root(
        path.normalize(path.join(dir, 'contracts', 'subdir', 'ava.sol'))))
        .to.equal(path.join('subdir', 'ava.sol'));
      // test networking bits
      expect(cfg.network_for_target(Target.Avalanche)).to.deep.equal({
        url: new URL('http://otherhost:9560'),
        autostart: false,
        num_nodes: 5,
        subnets: [],
      });
      const network_profile = cfg.network_profile();
      expect(network_profile.avalanche).to.deep.equal({
        url: new URL('http://otherhost:9560'),
        autostart: false,
        num_nodes: 5,
        subnets: [],
      });
      expect(network_profile.polygon?.url).to.deep.equal(new URL('http://localhost:9545'));
      expect(network_profile.ethereum?.url).to.deep.equal(new URL('http://localhost:7545'));
      // test private method
      const network_for_target_in_profile = _testExports.network_for_target_in_profile.bind(cfg);
      expect(network_for_target_in_profile('avalanche', 'default')).to.deep.equal({
        url: new URL('http://localhost:9560'),
        autostart: true,
        num_nodes: 5,
        subnets: [],
      });
      expect(network_for_target_in_profile('ethereum', 'default')?.url).to.deep.equal(
        new URL('http://otherhost:7545'));
      expect(network_for_target_in_profile('notreal', 'default')).to.be.undefined;
      expect(network_for_target_in_profile('avalanche', 'notreal')).to.be.undefined;

      const testnets = cfg.network_profile_by_name('testnets');
      expect(testnets.polygon?.url).to.deep.equal(
        new URL('https://rpc-mumbai.maticvigil.com'));
      expect(testnets.polygon?.proxy.chain_id).to.deep.equal(80001);
      expect(testnets.polygon?.proxy.port).to.deep.equal(9545);
      expect(testnets.polygon?.proxy.creds[0].mnemonic).to.deep.equal(<MnemonicConfig> {
        seed: { env: 'MY_MNEMONIC', },
        account_count: 2,
        derivation_path: 'm/44\'/60\'/0\'/0/',
      });
      expect(testnets.polygon?.proxy.creds[1].keystore).to.deep.equal(<KeystoreConfig>{
        file: '/foo/bar',
        password: <FileSecret> { file: '.secret', },
      });
      expect(testnets.polygon?.proxy.creds[2].private_key).to.deep.equal(<PrivateKeyConfig>{
        hex: <EnvVarSecret> { env: 'MY_PKEY', },
      });
    });
  });
  describe('bad config (bad project)', () => {
    it('does not create config', () => {
      expect(() =>
        Config.from_file(
          path.join(__dirname, 'fixtures', 'config-fixtures', 'bad-config-project.json')
        )
      ).to.throw(MalformedConfig);
    });
  });
  describe('bad config (bad project)', () => {
    it('does not create config', () => {
      expect(() =>
        Config.from_file(
          path.join(__dirname, 'fixtures', 'config-fixtures', 'bad-config-compiler.json')
        )
      ).to.throw(MalformedConfig);
    });
  });
  describe('bad config (contract paths)', () => {
    it('does not create config', () => {
      expect(() => {
        const c = Config.from_file(
          path.join(__dirname, 'fixtures', 'config-fixtures', 'bad-config-paths.json')
        );
        console.log(c);
      }
      ).to.throw(InvalidContractFilePaths);
    });
  });
  describe('bad config (missing network profile)', () => {
    it('does not create config', () => {
      expect(() =>
        Config.from_file(
          path.join(__dirname, 'fixtures',
            'config-fixtures', 'bad-config-missing-network-profile.json')
        )
      ).to.throw(MissingNetworkProfile);
    });
  });
});

describe('Config.nearest', () => {
  describe('good config', () => {
    const originalDir = cwd();
    const fixturesDir = path.join(__dirname, 'fixtures', 'config-fixtures');

    beforeAll(() => chdir(fixturesDir));
    afterAll(() => chdir(originalDir));

    it('create config', () => {
      // ensure current dir is the fixtures dir
      const dir = fs.realpathSync(fixturesDir);
      const cfg = Config.nearest();
      expect(cfg.type).to.equal(ProjType.Rust);
      expect(cfg.build_dir()).to.equal(path.normalize(path.join(dir, 'build')));
      expect(cfg.deploy_dir()).to.equal(path.normalize(path.join(dir, 'deploy')));
      const contracts = cfg.contracts();
      expect(contracts.root_dir).to.equal(path.normalize(path.join(dir, 'contracts')));
      expect(Object.fromEntries(contracts.targets)).to.deep.equal({
        avalanche: {
          files: [path.normalize(path.join(dir, 'contracts', 'ava.sol'))],
          compiler: Compiler.Solc,
        },
        ethereum: {
          files: [path.normalize(path.join(dir, 'contracts', 'eth.sol'))],
          compiler: Compiler.Solc,
        },
      });
      // test relative_to_root
      expect(contracts.relative_to_root(
        path.normalize(path.join(dir, 'contracts', 'subdir', 'ava.sol'))))
        .to.equal(path.join('subdir', 'ava.sol'));
      // test networking bits
      expect(cfg.network_for_target(Target.Avalanche)).to.deep.equal({
        url: new URL('http://localhost:9560'),
        autostart: true,
        num_nodes: 5,
        subnets: [],
      });
      expect(cfg.network_for_target(Target.Ethereum)?.url).to.deep.equal(
        new URL('http://localhost:8545'));
      expect(cfg.allow_import_from_external).to.equal(false);
      expect(cfg.contracts().import_dirs).to.deep.equal([
        path.normalize(path.join(dir, 'node_modules'))
      ]);
    });
  });
});

describe('PreCompileManifest.from_file', () => {
  describe('good manifest', () => {
    it('create manifest', () => {
      const json = {
        files: [
          { 'is_shim': false, 'rel_path': 'a.sol',
            'contract_dependencies': { 'A1': ['B1'], } as {[name: string]: string[]},
          },
          { 'is_shim': true, 'rel_path': 'b.sol',
            'contract_dependencies': { 'B1': [], 'B2': [], } as {[name: string]: string[]},
          }
        ],
      };
      const m = PreCompileManifest._from_json(json);
      expect(m.files).to.equal(json.files);
    });
  });
  describe('bad manifest (bad type)', () => {
    it('it throws', () => {
      const json = {
        files: [1],
      } as unknown as IPreCompileManifest; // deliberately bad case
      expect(() => PreCompileManifest._from_json(json)).to.throw(MalformedConfig);
    });
  });
  describe('bad manifest (bad subtype)', () => {
    it('it throws', () => {
      const json = {
        files: [{ abc: 123, }],
      } as unknown as IPreCompileManifest; // deliberately bad case
      expect(() => PreCompileManifest._from_json(json)).to.throw(MalformedConfig);
    });
  });
  describe('bad manifest (missing filed)', () => {
    it('it throws', () => {
      const json = {
      } as unknown as IPreCompileManifest; // deliberately bad case
      expect(() => PreCompileManifest._from_json(json)).to.throw(MalformedConfig);
    });
  });
  describe('bad manifest (extra filed)', () => {
    it('it throws', () => {
      const json = {
        files: [],
        bogus_field: {},
      } as unknown as IPreCompileManifest; // deliberately bad case
      expect(() => PreCompileManifest._from_json(json)).to.throw(MalformedConfig);
    });
  });
});
