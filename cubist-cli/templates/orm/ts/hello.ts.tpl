import {
  CubistORM,
  /* YourContractName, */
  Avalanche,
  Ethereum,
  Polygon,
} from '../build/orm/index.js';

// Project instance
const cubist = new CubistORM();

async function main() {
  console.log("Hello from Cubist dApp");
}
main().catch(console.error);

