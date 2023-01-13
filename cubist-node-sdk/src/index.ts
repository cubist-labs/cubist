/**
 * This module is the entry-point to all things Cubist. In particular, it
 * exports interfaces for:
 *
 * - Managing and working with Cubist projects and their smart contracts.
 *   - {@link Cubist} abstracts over cubist projects and is _the_ way to access
 *   {@link Contract | contracts} and {@link ContractFactory | contract
 *   factories}.
 *   - {@link ContractFactory} is for deploying contracts and contract shims.
 *   - {@link Contract} is for interacting with deployed contracts.
 * - Testing contracts.
 *   - {@link TestDK} (and its more specific variant {@link CubistTestDK})
 *   abstracts over a {@link Cubist} project's testing infrastructure. {@link
 *   TestDK} can be used for testing well-typed `CubistORM` projects too (see
 *   [Overview](/jsdoc/)).
 * - Marshalling configuration files (internal and read-only).
 *   - {@link Config} is the main interface for accessing project configuration.
 *
 * @module
 */
export * from './config';
export * from './cubist';
export * from './test';
