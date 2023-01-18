// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.16;

import './EthCounter.sol';

contract PolyCounter {

    EthCounter ethCounter;
    uint256 number;

    constructor (uint256 num, address ethCounterAddress) {
      number = num;
      ethCounter = EthCounter(ethCounterAddress);
    }

    function store(uint256 num) public {
        number = num;
        ethCounter.store(number);
    }

    function inc(uint256 num) public {
        number += num;
        ethCounter.store(number);
    }

    function dec(uint256 num) public {
      if (number >= num) {
        number -= num;
      } else {
        number = 0;
      }
      ethCounter.store(number);
    }

    function retrieve() public view returns (uint256){
      return number;
    }
}
