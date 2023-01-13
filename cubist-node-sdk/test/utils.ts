import path from 'path';
import * as dotenv from 'dotenv';
import { error, } from 'console';
import {
  PathBuf,
  find_file,
} from '../';

/**
 * Loads for a '.env' file from either a given directory or any of its parents, if found.
 * @param {PathBuf} dir Starting directory
 * @return {boolean} Whether a .env file was found
 */
export function dotenvNearest(dir: PathBuf): boolean {
  try {
    const dotenvFile = find_file('.env', dir);
    dotenv.config({ path: dotenvFile, });
    return true;
  } catch (FileNotFound) {
    return false;
  }
}

/**
 * Verbose logging.
 * @param {string} x - what to log
 */
export function verbose(x: string) {
  ((a) => a)(x); // ignore, appease linter
}

/**
 * Less verbose logging.
 * @param {string} x - what to log
 */
export function info(x: string) {
  error(x);
}

/**
 * Set the CUBIST_BIN environment variable to the path of the cubist
 * executable.
 */
export function setCubistBinToCargoBuildBin() {
  process.env.CUBIST_BIN = path.join(__dirname, '..', '..', 'target', 'debug', 'cubist');
}
