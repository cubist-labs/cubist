// SPDX-License-Identifier: MIT
// vim:syntax=javascript
pragma solidity ^0.8.0;

import "./TokenSender.sol";
import "./ERC20.sol";
import "./Context.sol";

/*
 * Bridge receiver.
 *
 * This contract is the "receiving" side of a cross-chain bridge.
 * It defines an ERC20 token that is minted in response to payments
 * on the "sending" side; a user can burn these tokens via bridgeSend
 * to release native tokens from the sending side.
 */
contract ERC20Bridged is Context, ERC20 {
    address private _bridge_owner;
    TokenSender private _bridge_sender; // XXX make this a TokenSender

    constructor(
        string memory name_,
        string memory symbol_,
        TokenSender sender_
    ) ERC20(name_, symbol_) {
        // make the _msgSender() account the person who controls the money
        _bridge_owner = _msgSender();

        // set up the "sender" side of the bridge
        _bridge_sender = sender_;
    }

    function bridgeMint(address to, uint256 amount) external {
        // auth
        require(_msgSender() == _bridge_owner, "unauthorized call");

        // mint functionality provided by the base ERC20 contract
        _mint(to, amount);
    }

    function bridgeSend(address to, uint256 amount) external {
        // burn functionality provided by the base ERC20 contract
        _burn(_msgSender(), amount);

        // transfer the requested amount
        _bridge_sender.bridgeReceive(to, amount);
    }
}
