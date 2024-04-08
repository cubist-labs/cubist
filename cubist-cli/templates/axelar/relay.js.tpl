'use strict';

const fs = require('fs');
const path = require('path');
const { setupNetwork, relay, evmRelayer } = require('@axelar-network/axelar-local-dev');

const RELAY_INTERVAL = 1000; // in millis

let gRelaying = false;
let gLastCallCount = 0;

// Prints 'callContract' relayer records collected since the last time
// this function was called. Returns `true` if anything was printed,
// `false` otherwise
function printDiff() {
    let props = Object.getOwnPropertyNames(evmRelayer.relayData.callContract);
    if (props.length > gLastCallCount) {
        let diff = {};
        for (let i = gLastCallCount; i < props.length; i++) {
            diff[props[i]] = evmRelayer.relayData.callContract[props[i]];
        }
        console.log({ 'callContract': diff });
        gLastCallCount = props.length;
        return true;
    }
    return false;
}

// Calls 'collectFeeds' on each 'gasReceiver' on each of the given chains
async function collectFees(chains) {
    for (let chain of chains) {
        await (await chain.gasReceiver.connect(chain.ownerWallet).collectFees(chain.ownerWallet.address, [])).wait();
    }
}

// Sets up an Axelar network and saves its info to a given file
async function setupAndSaveNetwork(url, name, ownerKey, outputFile) {
    let chain = await setupNetwork(url, { name: name, ownerKey: ownerKey });
    fs.mkdirSync(path.dirname(outputFile), { recursive: true });
    fs.writeFileSync(outputFile, JSON.stringify(chain.getCloneInfo(), undefined, '  '));
    return chain;
}

// Main relayer loop
async function run() {
    let chains = await Promise.all([
        {%- for c in chains %}
        setupAndSaveNetwork(
            '{{c.url}}',
            '{{c.name}}',
            '{{c.private_key}}',
            '{{c.output_file}}'),
        {% endfor %}
    ]);

    fs.writeFileSync('{{ready_file}}', 'ready');
    
    setInterval(async () => {
        if (gRelaying) return;
        gRelaying = true;
        await relay().catch(() => undefined);
        if (printDiff()) {
            await collectFees(chains);
        }
        gRelaying = false;
    }, RELAY_INTERVAL);
}

run();
