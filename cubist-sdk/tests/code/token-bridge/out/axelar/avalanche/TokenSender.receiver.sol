// SPDX-License-Identifier: MIT
pragma solidity ^0.8.16;
import {AxelarExecutable} from "@axelar-network/axelar-gmp-sdk-solidity/contracts/executable/AxelarExecutable.sol";
import {IAxelarGateway} from "@axelar-network/axelar-gmp-sdk-solidity/contracts/interfaces/IAxelarGateway.sol";
import {IAxelarGasService} from "@axelar-network/axelar-gmp-sdk-solidity/contracts/interfaces/IAxelarGasService.sol";
import "./ERC20Bridged.sol";
import "./Context.sol";
import "./TokenSender.sol";
contract TokenSenderInterface is AxelarExecutable {
    TokenSender _contractObject;
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
    constructor(address gateway) AxelarExecutable(gateway) {
        _owner = msg.sender;
    }
    function setTargetAddress(address contractAddress) public onlyOwner {
        _contractObject = TokenSender(contractAddress);
    }
    function _execute(
        string calldata,
        string calldata,
        bytes calldata payload
    ) internal override {
        (bool success,) = address(_contractObject).call(payload);
        require(success, "Calling target contract failed");
    }
}