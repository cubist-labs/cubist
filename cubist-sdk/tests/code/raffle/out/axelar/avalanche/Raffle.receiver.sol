// SPDX-License-Identifier: MIT
pragma solidity ^0.8.16;
import {AxelarExecutable} from "@axelar-network/axelar-gmp-sdk-solidity/contracts/executables/AxelarExecutable.sol";
import {IAxelarGateway} from "@axelar-network/axelar-gmp-sdk-solidity/contracts/interfaces/IAxelarGateway.sol";
import {IAxelarGasService} from "@axelar-network/axelar-gmp-sdk-solidity/contracts/interfaces/IAxelarGasService.sol";
import "@chainlink/contracts/src/v0.8/interfaces/VRFCoordinatorV2Interface.sol";
import "@chainlink/contracts/src/v0.8/VRFConsumerBaseV2.sol";
import "@chainlink/contracts/src/v0.8/interfaces/KeeperCompatibleInterface.sol";
import "./console.sol";
   

contract RaffleInterface is AxelarExecutable {
    IAxelarGasService public immutable gasReceiver;
    Raffle contractObject;

    constructor(
        address gateway_,
        address gasReceiver_,
        address contractAddress
    ) AxelarExecutable(gateway_) {
        gasReceiver = IAxelarGasService(gasReceiver_);
        contractObject = Raffle(contractAddress);
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

