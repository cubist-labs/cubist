// SPDX-License-Identifier: MIT
pragma solidity ^0.8.17;

contract Used {

uint256 number; 

enum Integer {
    ONE,
    TWO,
    THREE
}

function store(Integer num) public {
    if (num == Integer.ONE) {
        number = 1;
    }
    if (num == Integer.TWO) {
        number = 2;
    }
    if (num == Integer.THREE) {
        number = 3;
    }
}

}