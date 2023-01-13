// SPDX-License-Identifier: MIT
pragma solidity >=0.6.2;
import './Used.sol';

contract User {

    Used used; 

    function mint(address to, uint256 amount) public {
        used.mint(to, amount);
    }
}