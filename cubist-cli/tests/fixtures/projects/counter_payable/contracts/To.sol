// SPDX-License-Identifier: MIT
pragma solidity ^0.8.9;

contract To {
    uint256 _num;

    constructor() {
    }

    function store(uint256 num) public payable {
        _num = num;
    }

    function retrieve() public view returns (uint256) {
        return _num;
    }
}
