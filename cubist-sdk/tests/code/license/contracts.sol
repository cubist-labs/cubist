// SPDX-License-Identifier: GPL-3.0

pragma solidity >=0.7.0 <0.9.0;

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

    function inc(uint256 num) public {
        number += num;
        ethStorage.store(number);
    }

    function dec(uint256 num) public {
      if (number >= num) {
        number -= num;
      } else {
        number = 0;
      }
      ethStorage.store(number);
    }

    function retrieve() public view returns (uint256){
      return number;
    }
}

contract EthStorage {

    uint256 number;

    constructor (uint256 num) {
      number = num;
    }

    function store(uint256 num) public {
      number = num;
    }

    function retrieve() public view returns (uint256){
        return number;
    }
}