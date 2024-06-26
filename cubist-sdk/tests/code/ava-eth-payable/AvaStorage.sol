// SPDX-License-Identifier: MIT
pragma solidity ^0.8.17;

import './EthStorage.sol';

contract AvaStorage {

    EthStorage ethStorage;
    uint256 number;

    constructor (uint256 num) {
      number = num;
    }

    function store(uint256 num) public {
      number = num;
      ethStorage.store(number);
    }

    function storePayable(uint256 num) public payable {
      number = num;
      ethStorage.storePayable{value: msg.value}(number);
    }

    function retrieve() public view returns (uint256){
      return number;
    }
}