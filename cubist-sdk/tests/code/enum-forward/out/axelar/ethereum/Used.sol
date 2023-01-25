// SPDX-License-Identifier: MIT
pragma solidity ^0.8.16;
import {AxelarExecutable} from "@axelar-network/axelar-gmp-sdk-solidity/contracts/executables/AxelarExecutable.sol";
import {IAxelarGateway} from "@axelar-network/axelar-gmp-sdk-solidity/contracts/interfaces/IAxelarGateway.sol";
import {IAxelarGasService} from "@axelar-network/axelar-gmp-sdk-solidity/contracts/interfaces/IAxelarGasService.sol";
   
contract Used is AxelarExecutable {
    enum Integer {
    ONE,
    TWO,
    THREE
}
    
    IAxelarGasService public immutable gasReceiver;
    
    // The receiver interface address
    string interfaceAddress;
    constructor(
        address gateway_,
        address gasReceiver_,
        string memory interfaceAddress_
    ) AxelarExecutable(gateway_) {
        gasReceiver = IAxelarGasService(gasReceiver_);
        interfaceAddress = interfaceAddress_;
    }
    
    function store(Integer num) external payable {
      bytes memory payload = abi.encodeWithSignature("store(Integer)", num);
      gasReceiver.payNativeGasForContractCall{value: msg.value}(
            address(this),
            "avalanche",
            interfaceAddress,
            payload,
            msg.sender
        );
        gateway.callContract("avalanche", interfaceAddress, payload);
    }
    
}