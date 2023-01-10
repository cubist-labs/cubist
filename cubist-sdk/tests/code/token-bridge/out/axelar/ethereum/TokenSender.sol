// SPDX-License-Identifier: MIT
pragma solidity ^0.8.16;
import {AxelarExecutable} from "@axelar-network/axelar-gmp-sdk-solidity/contracts/executables/AxelarExecutable.sol";
import {IAxelarGateway} from "@axelar-network/axelar-gmp-sdk-solidity/contracts/interfaces/IAxelarGateway.sol";
import {IAxelarGasService} from "@axelar-network/axelar-gmp-sdk-solidity/contracts/interfaces/IAxelarGasService.sol";
import "./ERC20Bridged.sol";
import "./Context.sol";
   

contract TokenSender is AxelarExecutable {
    error TransactionTooSmall();
    
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

    
    function bridgeReceive(address to, uint256 amount) external payable {
      bytes memory payload = abi.encodeWithSignature("bridgeReceive(to, amount)", address, uint256);
      gasReceiver.payNativeGasForContractCall{value: msg.value}(
            address(this),
            "avalanche",
            interfaceAddress,
            payload,
            msg.sender
        );  
    }
    
}

