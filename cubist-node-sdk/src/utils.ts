import path from 'path';
import * as fs from 'fs';
import { FileNotFound, PathBuf, } from './config';

/**
 * Find file starting from directory.
 * @param {PathBuf} name Config filename.
 * @param {PathBuf} dir starting directory
 * @return {PathBuf} the file path.
 * @throws {FileNotFound} if no file is found.
 */
export function find_file(name: PathBuf, dir: PathBuf): PathBuf {
  const candidate = path.join(dir, name);
  // is candidate a file
  if (fs.existsSync(candidate) && fs.statSync(candidate).isFile()) {
    return candidate;
  }

  const parentDir = path.join(dir, '..');

  // If we're at the root, throw an error
  if (parentDir === dir) {
    throw new FileNotFound(name);
  }

  return find_file(name, parentDir);
}
