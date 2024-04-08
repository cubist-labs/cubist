// SPDX-License-Identifier: MIT
pragma solidity ^0.8.7;
import "@chainlink/contracts/src/v0.8/interfaces/VRFCoordinatorV2Interface.sol";
import "@chainlink/contracts/src/v0.8/VRFConsumerBaseV2.sol";
import "@chainlink/contracts/src/v0.8/interfaces/KeeperCompatibleInterface.sol";
import "./console.sol";
contract Raffle {
    enum RaffleState {
    OPEN,
    CALCULATING
}
    event RequestedRaffleWinner (uint256 indexed requestId);
    event RaffleEnter (address indexed player);
    event WinnerPicked (address indexed player);
    
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
    constructor() {
        _owner = msg.sender;
    }
    event __cubist_event_Raffle_enterRaffle();
    function enterRaffle() public onlyCaller payable {
        emit __cubist_event_Raffle_enterRaffle();
    }
    
}