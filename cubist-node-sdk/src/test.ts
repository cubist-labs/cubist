/**
 * This module exports a simple testing framework.
 *
 * Specifically, it exposes the {@link TestDK} class (and its more specific
 * variant {@link CubistTestDK}) which abstracts over a {@link cubist.Cubist}
 * project's testing infrastructure. {@link TestDK} can be used to:
 *
 * - {@link TestDK.startService | Start} and {@link TestDK.stopService | stop}
 *   local [chains and
 *   relayer](/guide/quick-start#start-cubist-chains-and-relayer), e.g., before
 *   and, respectively, after running tests.
 * - {@link TestDK.build | Build} your app, much like [cubist
 *   build](/guide/quick-start#build-the-app)
 * - Test projects in fresh (temporary) `build` and `deploy` directories.
 *
 * {@link TestDK} can be used for testing well-typed [CubistORM] projects too
 * (see [Overview](../)).
 *
 * [CubistORM]: /jsdoc-md/cubist.CubistORM
 *
 * @module
 */
import { Config, } from './config';
import { Cubist, } from './cubist';
import * as path from 'path';
import { emptyDir, } from 'fs-extra';
import { tmpdir, } from 'os';
import { spawn, ChildProcess, SpawnOptions, } from 'child_process';
import { sync as which, } from 'which';
import { mkdtempSync, } from 'fs';

/**
 * Options for creating a TestDK instance.
 *
 * @group Testing
 * */
export interface TestDKOptions {
  /** Use temporary directory for build_dir. */
  tmp_build_dir?: boolean;
  /** Use temporary directory for deploy_dir. */
  tmp_deploy_dir?: boolean;
  /** Arguments to call the {@link cubist.Cubist}-like constructor with. */
  args?: any[]; // eslint-disable-line @typescript-eslint/no-explicit-any
}

/**
 * Cubist service we can start/stop.
 *
 * @group Testing
 * */
export enum Service {
  /** All target chains */
  Chains = 'chains', // eslint-disable-line no-unused-vars
  /** Relayer */
  Relayer = 'relayer' // eslint-disable-line no-unused-vars
}

/** Cubist class reference. We define this type to restrict the kinds of
 * constructors {@link TestDK} can be used for. This constructor is called to
 * create a new project, so anything that extends {@link cubist.Cubist} is
 * sufficient.
 *
 * @group Testing
 * */
export type CubistClassRef<T extends Cubist> = { new (config?: Config): T };

/**
 * Class that abstracts over a Cubist project's testing process. In particular,
 * this class makes it possible to (1) create {@link cubist.Cubist} (or
 * [CubistORM]) projects with temporary build and deploy directories and (2)
 * start and stop {@link Service | services} when running tests.
 *
 * @example
 *
 * Using {@link TestDK} to test [CubistORM] projects with
 * [Jest](https://jestjs.io/). The `cubist build` command generates [CubistORM]
 * at build time; this class extends {@link cubist.Cubist} with
 * project-specific contracts (see the [Overview](/jsdoc)).
 *
 * ```typescript
 * import { TestDK, } from '@cubist-alpha/cubist';
 * import { CubistORM } from '../build/orm';
 *
 * const testdk = new TestDK<CubistORM>(CubistORM, { tmp_deploy_dir: true });
 *
 * beforeAll(async () => {
 *   await testdk.startService();
 * });
 *
 * afterAll(async () => {
 *   await testdk.stopService();
 * });
 *
 * beforeEach(async () => {
 *   await testdk.emptyDeployDir();
 * });
 *
 *
 * // get the project under test
 * const cubist = testdk.cubist;
 *
 * // .. use cubist to get contracts and contract factories, as usual ...
 * ```
 *
 * In this example, we create a new testing [CubistORM] project whose `deploy`
 * directory is set to a new temporary directory. This lets you test your
 * project without clobbering existing deploy receipts.
 *
 * We start services---chains and relayer---before all tests, and stop them at
 * the end.
 *
 * We also empty the `deploy` directory before each test.
 *
 * [CubistORM]: /jsdoc-md/cubist.CubistORM
 *
 * @group Testing
 * */
export class TestDK<T extends Cubist> {
  /** The underlying project. When extending this code, in general using config
   * instead of _cubist is the right move (since we use TestDK for thesting
  * Configs too). */
  private readonly _cubist: T;
  private readonly _config: Config;
  private services: ChildProcess | Map<Service, ChildProcess> | null;
  private custom_build_dir?: string;
  private custom_deploy_dir?: string;

  /** Create new instance of TestDK.
   *
   * @example
   * Testing project in fresh deploy directory.
   * ```typescript
   * const testdk = new TestDK<CubistORM>(CubistORM, { tmp_deploy_dir: true });
   * ```
   *
   * @example
   * Testing project in fresh build and deploy directory.
   * ```typescript
   * const testdk = new TestDK<Cubist>(Cubist, { tmp_build_dir: true, tmp_deploy_dir: true });
   *
   * beforeAll(async () => {
   *   // build the project
   *   await testdk.build();
   *   // start chains and relayer
   *   await testdk.startService();
   * });
   * ```
   *
   * @example
   * Testing already-built (and deployed) project.
   * ```typescript
   * const testdk = new TestDK<Cubist>(Cubist);
   * ```
   *
   * @param {CubistClassRef<T>} CubistX - Reference to Cubist class.
   * @param {TestDKOptions?} options? - Options for creating instance.
   */
  constructor(CubistX: CubistClassRef<T>, options?: TestDKOptions) {
    if (options?.args) {
      this._cubist = new CubistX(...options.args);
    } else {
      this._cubist = new CubistX();
    }
    this._config = this._cubist.config;
    if (options?.tmp_build_dir) {
      const tmp = mkdtempSync(path.join(tmpdir(), 'cubist-node-sdk-test-build-'));
      this._config.__set_build_dir(tmp);
      this.custom_build_dir = tmp;
    }
    if (options?.tmp_deploy_dir) {
      const tmp = mkdtempSync(path.join(tmpdir(), 'cubist-node-sdk-test-deploy-'));
      this._config.__set_deploy_dir(tmp);
      this.custom_deploy_dir = tmp;
    }
    this.services = null;
  }

  /** Get the cubist project we're testing.
   *
   * ::: warning
   * Whenever you use {@link TestDK}, you should always use this
   * accessors to get a reference to the {@link cubist.Cubist} project.
   * Creating a new {@link cubist.Cubist} object could have unwanted behavior.
   * Here is an example how things can go wrong:
   * :::
   *
   * ```typescript
   * const testdk = new TestDK<CubistORM>(CubistORM, { tmp_deploy_dir: true });
   * const cubist = testdk.cubist; // this is correct
   *
   * const bad_cubist = new Cubist(); // this is not what you want; the deploy
   * // directory for this project is the deploy directory specified in the config
   * // that is:
   * assert(cubist.config.build_dir() != bad_cubist.config.build_dir());
   * ```
   *
   * */
  get cubist() {
    return this._cubist;
  }

  /** Get the underlying config. */
  get config() {
    return this._config;
  }

  /** Execute cubist command with given args. The cubist executable itself can
   * be set by setting the CUBIST_BIN environment variable.
   * @param {string} cmd - Command to execute.
   * @param {string[]} args? - Optional arguments to pass to command.
   * @return {Promise<ChildProcess>} - Promise that resolves to child process.
   */
  private async cubistExec(cmd: string, args?: string[]): Promise<ChildProcess> {
    const config_file = this._config.config_path;
    const options:SpawnOptions = { stdio: 'inherit', env: { ...process.env, }, };
    if (this.custom_build_dir) {
      options.env.CUBIST_BUILD_DIR = this.custom_build_dir;
    }
    if (this.custom_deploy_dir) {
      options.env.CUBIST_DEPLOY_DIR = this.custom_deploy_dir;
    }
    const cubist_exe = process.env.CUBIST_BIN ??
      which('cubist', { nothrow: true, }) ?? 'cubist';

    const child = spawn(cubist_exe,
      [cmd, '--config', config_file, ...args || []], options);

    return new Promise((resolve, reject) => {
      child.on('error', reject);
      child.on('exit', (code) => {
        if (code === 0) {
          resolve(child);
        } else {
          reject(new Error(`cubist exited with code ${code}`));
        }
      });
    });
  }

  /**
   * Start particular service (or all)---namely [chains and
   * relayer](/guide/quick-start#start-cubist-chains-and-relayer)---with
   * [`cubist start`](/guide/cli#cubist-start).
   *
   * @param {Service?} service - Service to start.
   */
  async startService(service?: Service) {
    if (service) {
      // initialize services if not already done
      if (this.services === null) {
        this.services = new Map<Service, ChildProcess>();
      }
      if (!(this.services instanceof Map) || this.services.has(service)) {
        throw new Error(`Cannot start service ${service}; already started.`);
      }
      this.services.set(service, await this.cubistExec('start', [service]));
    } else {
      if (this.services !== null) {
        throw new Error('Cannot start all services; already started.');
      }
      this.services = await this.cubistExec('start');
    }
  }

  /**
   * Stop particular service (or all)---namely [chains and
   * relayer](/guide/quick-start#start-cubist-chains-and-relayer)---with
   * [`cubist start`](/guide/cli#cubist-start).
   *
   * @param {Services?} service - Service to stop.
   */
  async stopService(service?: Service) {
    if (service) {
      if (this.services instanceof Map && this.services.has(service)) {
        // we can be more permissive and stop a particular service even if we
        // started all services, but no need to make this more complex for now.
        await this.cubistExec('stop', [service]);
        this.services.delete(service);
      }
    } else {
      await this.cubistExec('stop');
      this.services = null;
    }
  }

  /** Build project, i.e., run [`cubist build`](/guide/cli#cubist-build) */
  async build() {
    await this.cubistExec('build');
  }

  /** Clobber the project deploy directory. */
  async emptyDeployDir() {
    await emptyDir(this._config.deploy_dir());
  }

  /**
   * Stop service(s) on process exit.
   * @param {Services?} service - Service to stop.
   */
  stopServiceOnExit(service?: Service) {
    const stop = () => {
      this.stopService(service).catch(console.error);
    };
    process.on('exit', stop);
    process.on('SIGINT', stop);
    process.on('SIGUSR1', stop);
    process.on('SIGUSR2', stop);
    process.on('SIGTERM', stop);
    process.on('uncaughtException', stop);
  }
}

/**
 * Wrapper for {@link TestDK} specific to {@link cubist.Cubist}, eliding the
 * need for (1) a type parameter and (2) passing {@link cubist.Cubist} as an
 * argument.
 *
 * @example
 * Testing already-built (and deployed) project.
 * ```typescript
 * const testdk = new CubistTestDK();
 * ```
 *
 * @group Testing
 */
export class CubistTestDK extends TestDK<Cubist> {
  /** Create new instance of CubistTestDK.
   * @param {TestDKOptions?} options? - Options for creating instance.
   **/
  constructor(options?: TestDKOptions) {
    super(Cubist, options);
  }
}

/** @internal Class for testing the Config class. This only extends Cubist to
 * make type checker happy; it doesn't actually use any of the Cubist
 * functionality.
 */
class TestCubistConfig extends Cubist {
  /** Create new instance of TestCubistConfig.
   * @param {Config} config - Config to test.
   **/
  constructor(config: Config) {
    super(config);
  }
}

/** @internal Options for creating a ConfigTestDK instance. This is a subset of
 * `TestDKOptions` (without args).*/
export interface ConfigTestDKOptions {
  /** Use tmporary directory for build_dir. */
  tmp_build_dir?: boolean;
  /** Use tmporary directory for deploy_dir. */
  tmp_deploy_dir?: boolean;
}

/** @internal Wrapper for TestDK specific to Config. */
export class ConfigTestDK extends TestDK<TestCubistConfig> {
  /** Create a new ConfigTestDK instance.
   * @param {string} file - Path to config file.
   * @param {ConfigTestDKOptions?} options - Options for creating instance.
   */
  constructor(file: string, options?: ConfigTestDKOptions) {
    const cfg = Config.from_file(file);
    super(TestCubistConfig, { args: [cfg], ...options, });
  }
}
