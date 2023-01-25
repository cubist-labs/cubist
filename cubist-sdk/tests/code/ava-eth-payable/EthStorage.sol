// SPDX-License-Identifier: MIT
pragma solidity ^0.8.17;

contract EthStorage {

    uint256 number;

    constructor (uint256 num) {
      number = num;
    }

    function store(uint256 num) public payable {
      number = num;
    }

    function retrieve() public view returns (uint256){
        return number;
    }
}