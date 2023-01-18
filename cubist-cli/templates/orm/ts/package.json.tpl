{
  "name": "{{name}}",
  "author": "{{author}}",
  "version": "0.0.1",
  "description": "TypeScript Web3 Cubist App",
  "type": "module",
  "main": "dist/index.js",
  "types": "dist/index.d.ts",
  "files": [
    "contracts/**",
    "src/**"
  ],
  "dependencies": {
    "@cubist-alpha/cubist": "github:cubist-alpha/cubist-node-sdk"
  },
  "devDependencies": {
    "@types/node": "^18.7.23",
    "ts-node": "^10.9.1",
    "typescript": "^4.9.3"
  },
  "scripts": {
    "build": "tsc"
  },
  "keywords": [
    "web3",
    "cubist"
  ],
  "license": "UNLICENSED"
}
