// SPDX-License-Identifier: MIT
pragma solidity ^0.8.16;
import {AxelarExecutable} from "@axelar-network/axelar-gmp-sdk-solidity/contracts/executables/AxelarExecutable.sol";
import {IAxelarGateway} from "@axelar-network/axelar-gmp-sdk-solidity/contracts/interfaces/IAxelarGateway.sol";
import {IAxelarGasService} from "@axelar-network/axelar-gmp-sdk-solidity/contracts/interfaces/IAxelarGasService.sol";
import "@chainlink/contracts/src/v0.8/interfaces/VRFCoordinatorV2Interface.sol";
import "@chainlink/contracts/src/v0.8/VRFConsumerBaseV2.sol";
import "@chainlink/contracts/src/v0.8/interfaces/KeeperCompatibleInterface.sol";
import "./console.sol";
   
contract Raffle is AxelarExecutable {
    enum RaffleState {
    OPEN,
    CALCULATING
}
    event RequestedRaffleWinner (uint256 indexed requestId);
    event RaffleEnter (address indexed player);
    event WinnerPicked (address indexed player);
    
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
    
    function enterRaffle() external payable {
      bytes memory payload = abi.encodeWithSignature("enterRaffle()");
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