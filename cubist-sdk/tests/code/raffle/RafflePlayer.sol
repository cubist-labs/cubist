// SPDX-License-Identifier: MIT
pragma solidity ^0.8.7;

import './Raffle.sol';

contract RafflePlayer {
    Raffle raffle;

    function enter() public payable {
        raffle.enterRaffle();
    }

}