// SPDX-License-Identifier: GPL-3.0

pragma solidity >=0.7.0 <0.9.0;

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