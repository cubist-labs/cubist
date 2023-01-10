// SPDX-License-Identifier: MIT
pragma solidity ^0.8.16;
import {AxelarExecutable} from "@axelar-network/axelar-gmp-sdk-solidity/contracts/executables/AxelarExecutable.sol";
import {IAxelarGateway} from "@axelar-network/axelar-gmp-sdk-solidity/contracts/interfaces/IAxelarGateway.sol";
import {IAxelarGasService} from "@axelar-network/axelar-gmp-sdk-solidity/contracts/interfaces/IAxelarGasService.sol";
import "@openzeppelin/contracts@4.6.0/token/ERC721/ERC721.sol";
import "@openzeppelin/contracts@4.6.0/token/ERC721/extensions/ERC721URIStorage.sol";
import "@openzeppelin/contracts@4.6.0/access/Ownable.sol";
import "@openzeppelin/contracts@4.6.0/utils/Counters.sol";
   

contract DynNFT is AxelarExecutable {
    using Counters for Counters.Counter;
    
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

    
    function growFlower(uint256 _tokenId) external payable {
      bytes memory payload = abi.encodeWithSignature("growFlower(_tokenId)", uint256);
      gasReceiver.payNativeGasForContractCall{value: msg.value}(
            address(this),
            "polygon",
            interfaceAddress,
            payload,
            msg.sender
        );  
    }
    
}

