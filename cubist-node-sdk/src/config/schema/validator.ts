import Ajv from 'ajv';
import addFormats from 'ajv-formats';

import { MalformedConfig, } from '../errors';

import configSchema from './config.schema.json';
import preCompileManifestSchema from './pre_compile_manifest.schema.json';

// Extend the validator with custom formats we need

/** Validate uint16.
  * @param {number} n - the number to validate.
  * @return {boolean} true if valid. */
function validateUint16(n: number) {
  return Number.isSafeInteger(n) && n >= 0 && n < 2 ** 16;
}

/** Validate uint32.
  * @param {number} n - the number to validate.
  * @return {boolean} true if valid. */
function validateUint32(n: number) {
  return Number.isSafeInteger(n) && n >= 0 && n < 2 ** 32;
}

// Create schema validator that also set default values from schema.
const ajv = new Ajv({
  useDefaults: true,
  // ajv's support for defaults within more complex schema is not great, so we
  // can't use strict mode for now.
  strict: false,
});
addFormats(ajv);
ajv.addFormat('uint16', {
  type: 'number',
  validate: validateUint16,
});
ajv.addFormat('uint32', {
  type: 'number',
  validate: validateUint32,
});

const validate_config = ajv.compile(configSchema);
const validate_pre_compile_manifest = ajv.compile(preCompileManifestSchema);

/** Validate a config object.
 * @param {any} config - the config object to validate.
 * @throws {MalformedConfig} if the config is invalid. */
export function validateConfig(config) {
  if (!validate_config(config)) {
    throw new MalformedConfig(ajv.errorsText(validate_config.errors));
  }
}

/** Validate a pre-compile manifest object.
 * @param {any} manifest - the manifest object to validate.
 * @throws {MalformedConfig} if the manifest is invalid. */
export function validatePreCompileManifest(manifest) {
  if (!validate_pre_compile_manifest(manifest)) {
    throw new MalformedConfig(ajv.errorsText(validate_pre_compile_manifest.errors));
  }
}
