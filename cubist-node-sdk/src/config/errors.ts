/**
* Errors raised by this package when loading configurations.
* @internal
**/
export class ConfigError extends Error {
  /** Configuration error.
    * @param {string} message - Error message. */
  constructor(message: string) {
    super(message);
    this.name = 'ConfigError';
  }
}

/**
* Error raised when deserialization fails.
* @internal
**/
export class MalformedConfig extends ConfigError {
  /** Malformed config.
   * @param {string} message - JSON or AJV error message. */
  constructor(message: string) {
    super(`Malformed config: ${message}`);
    this.name = 'MalformedConfig';
  }
}

