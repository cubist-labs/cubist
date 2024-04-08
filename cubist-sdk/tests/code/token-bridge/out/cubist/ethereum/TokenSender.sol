// SPDX-License-Identifier: MIT
pragma solidity ^0.8.0;
import "./ERC20Bridged.sol";
import "./Context.sol";
contract TokenSender {
    error TransactionTooSmall();
    
    address private _owner;
    mapping(address => bool) private _callers;
    function _isOwner(address account) private view returns (bool) {
        return _owner == account;
    }
    function _isCaller(address account) private view returns (bool) {
        return _callers[account];
    }
    modifier onlyOwner() {
        require(_isOwner(msg.sender), "Cubist: sender is not the owner");
        _;
    }
    modifier onlyCaller() {
        require(_isCaller(msg.sender), "Cubist: sender is not a caller");
        _;
    }
    function approveCaller(address account) public onlyOwner {
        _callers[account] = true;
    }
    constructor() {
        _owner = msg.sender;
    }
    event __cubist_event_TokenSender_bridgeReceive(address to, uint256 amount);
    function bridgeReceive(address to, uint256 amount) public onlyCaller {
        emit __cubist_event_TokenSender_bridgeReceive(to, amount);
    }
    
}