// SPDX-License-Identifier: MIT
pragma solidity ^0.8.16;
import {AxelarExecutable} from "@axelar-network/axelar-gmp-sdk-solidity/contracts/executables/AxelarExecutable.sol";
import {IAxelarGateway} from "@axelar-network/axelar-gmp-sdk-solidity/contracts/interfaces/IAxelarGateway.sol";
import {IAxelarGasService} from "@axelar-network/axelar-gmp-sdk-solidity/contracts/interfaces/IAxelarGasService.sol";
   

contract Used is AxelarExecutable {
    struct Book {
    string name;
    string author;
    uint256 numPages;
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

    
    function addBook(Book book) external payable {
      bytes memory payload = abi.encodeWithSignature("addBook(book)", Book);
      gasReceiver.payNativeGasForContractCall{value: msg.value}(
            address(this),
            "ethereum",
            interfaceAddress,
            payload,
            msg.sender
        );  
    }
    
}

