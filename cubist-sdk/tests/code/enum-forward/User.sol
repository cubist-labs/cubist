// SPDX-License-Identifier: MIT
pragma solidity ^0.8.17;

import './Used.sol';

contract User {
    Used used;

    function store_remote(Used.Integer num) public {
    	Used.Integer my_int = Used.Integer.ONE; 
        used.store(num);
    }

}