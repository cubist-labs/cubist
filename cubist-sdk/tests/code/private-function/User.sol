// SPDX-License-Identifier: MIT
pragma solidity ^0.8.7;

contract User {

    Used used; 

    function getVal() public view returns (uint256) {
        return used.get();
    }
}