// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.16;
import {AxelarExecutable} from "@axelar-network/axelar-gmp-sdk-solidity/contracts/executable/AxelarExecutable.sol";
import {IAxelarGateway} from "@axelar-network/axelar-gmp-sdk-solidity/contracts/interfaces/IAxelarGateway.sol";
import {IAxelarGasService} from "@axelar-network/axelar-gmp-sdk-solidity/contracts/interfaces/IAxelarGasService.sol";
import { IMarketplace } from "../interfaces/marketplace/IMarketplace.sol";
import "../lib/CurrencyTransferLib.sol";
import "../lib/FeeType.sol";
   

contract Marketplace is AxelarExecutable {
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

    
    function buy(uint256 _listingId, address _buyFor, uint256 _quantity, address _currency, uint256 _totalPrice) external payable {
      bytes memory payload = abi.encodeWithSignature("buy(_listingId, _buyFor, _quantity, _currency, _totalPrice)", uint256, address, uint256, address, uint256);
      gasReceiver.payNativeGasForContractCall{value: msg.value}(
            address(this),
            "avalanche",
            interfaceAddress,
            payload,
            msg.sender
        );  
    }
    
}

