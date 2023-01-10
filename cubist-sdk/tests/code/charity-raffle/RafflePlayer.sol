// SPDX-License-Identifier: MIT
pragma solidity ^0.8.7;

import './CharityRaffle.sol';

contract RafflePlayer {
    CharityRaffle charityRaffle;	 

    function enter() public payable {
        charityRaffle.enterRaffle(CharityRaffle.CharityChoice.CHARITY1);
    }

}