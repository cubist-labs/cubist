// SPDX-License-Identifier: GPL-3.0
pragma solidity >=0.7.0 <0.9.0;
contract EthStorage {
    
    event __cubist_event_EthStorage_store(uint256 num);
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
    constructor() {
        _owner = msg.sender;
    }
    function approveCaller(address account) public onlyOwner {
        _callers[account] = true;
    }
	
    function store(uint256 num) public onlyCaller {
        emit __cubist_event_EthStorage_store(num);
    }
}