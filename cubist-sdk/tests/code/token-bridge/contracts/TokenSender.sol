// SPDX-License-Identifier: MIT
// vim:syntax=javascript
pragma solidity ^0.8.0;

import "./ERC20Bridged.sol";
import "./Context.sol";

/*
 * Bridge sender
 *
 * This contract is the "sending" side of a cross-chain bridge.
 * It receives payment in native tokens and issues ERC20 tokens
 * from ERC20Bridged in response. When commanded by ERC20Bridged,
 * it releases native tokens to a specified recipient.
 */
contract TokenSender is Context {
    address private _bridge_owner;
    ERC20Bridged private _bridge_receiver;

    error TransactionTooSmall();

    constructor(ERC20Bridged receiver_) {
        // make the _msgSender() account the person who controls the money
        _bridge_owner = _msgSender();

        // set up the "receiver" side of the bridge
        _bridge_receiver = receiver_;
    }

    function bridgeSend(address to) public payable {
        // minimum transaction size
        if (msg.value < 1000 gwei) revert TransactionTooSmall();

        uint256 kept = msg.value / 1000;  // keep ~0.1% as a fee
        uint256 to_send = msg.value - kept;

        _bridge_receiver.bridgeMint(to, to_send);
    }

    function bridgeReceive(address to, uint256 amount) external {
        // only accept this command from the owner
        require(_msgSender() == _bridge_owner, "unauthorized call");

        // transfer the requested amount
        payable(to).transfer(amount);
    }
}
