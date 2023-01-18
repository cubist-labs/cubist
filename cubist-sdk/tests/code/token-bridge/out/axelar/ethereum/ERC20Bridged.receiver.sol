// SPDX-License-Identifier: MIT
pragma solidity ^0.8.16;
import {AxelarExecutable} from "@axelar-network/axelar-gmp-sdk-solidity/contracts/executables/AxelarExecutable.sol";
import {IAxelarGateway} from "@axelar-network/axelar-gmp-sdk-solidity/contracts/interfaces/IAxelarGateway.sol";
import {IAxelarGasService} from "@axelar-network/axelar-gmp-sdk-solidity/contracts/interfaces/IAxelarGasService.sol";
import "./TokenSender.sol";
import "./ERC20.sol";
import "./Context.sol";
   

contract ERC20BridgedInterface is AxelarExecutable {
    IAxelarGasService public immutable gasReceiver;
    ERC20Bridged contractObject;

    constructor(
        address gateway_,
        address gasReceiver_,
        address contractAddress
    ) AxelarExecutable(gateway_) {
        gasReceiver = IAxelarGasService(gasReceiver_);
        contractObject = ERC20Bridged(contractAddress);
    }

    function _execute(
        string calldata,
        string calldata,
        bytes calldata payload_
    ) internal override {
        (bool success,) = address(contractObject).call(payload_);
        require(success);
    }
}

