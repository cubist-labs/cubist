// SPDX-License-Identifier: MIT
pragma solidity ^0.8.16;
import {AxelarExecutable} from "@axelar-network/axelar-gmp-sdk-solidity/contracts/executable/AxelarExecutable.sol";
import {IAxelarGateway} from "@axelar-network/axelar-gmp-sdk-solidity/contracts/interfaces/IAxelarGateway.sol";
import {IAxelarGasService} from "@axelar-network/axelar-gmp-sdk-solidity/contracts/interfaces/IAxelarGasService.sol";
contract Used is AxelarExecutable {
    enum Integer {
    ONE,
    TWO,
    THREE
}
    
    IAxelarGasService public immutable _gasReceiver;
    // The receiver interface address
    string _interfaceAddress;
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
    constructor(
        address gateway,
        address gasReceiver,
        string memory interfaceAddress
    ) AxelarExecutable(gateway) {
        _gasReceiver = IAxelarGasService(gasReceiver);
        _interfaceAddress = interfaceAddress;
        _owner = msg.sender;
    }
    
    function store(Integer num) external payable onlyCaller {
      bytes memory payload = abi.encodeWithSignature("store(Integer)", num);
      _gasReceiver.payNativeGasForContractCall{value: msg.value}(
            address(this),
            "Avalanche",
            _interfaceAddress,
            payload,
            msg.sender
        );
        gateway.callContract("Avalanche", _interfaceAddress, payload);
    }
    
}