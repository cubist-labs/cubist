This directory has a collection of utilities. Use them at your own risk.

## [Combine NPM Dependabot PRs](./combine-npm-dependabot.sh)

Tries to automatically apply dependabot PRs to our
[package.json](../cubist-node-sdk/package.json). Example usage:

```
git fetch # get all remote branches
./combine-npm-dependabot.sh
pushd ./npm-patches
# modify each remaining .patch file
# or just modify the sdk package.json and commit your changes
popd
# update the lock file
pushd ../cubist-node-sdk && yarn && git add yarn.lock
git commit -am "update yarn.lock"
```
