# Cubist node SDK

## Hacking on the SDK

The config is based on our Rust SDK Config. Hence, when you're hacking on this
you might need to update the config schema:

```
yarn gen-schema
```

Before creating a PR, you should run the formatter, linter, and tests:

```
yarn fmt
yarn lint
yarn test
```
