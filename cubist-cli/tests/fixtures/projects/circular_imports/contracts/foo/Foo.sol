// SPDX-License-Identifier: MIT
pragma solidity ^0.8.9;

import "../bar/Bar.sol";
import "../Common.sol";

contract Foo {
    Bar private _bar;
    uint256 _num;

    constructor(Bar bar) {
        _bar = bar;
    }

    function call_bar(uint256 num) public {
        _bar.store(num);
    }

    function store(uint256 num) public {
        _num = num;
    }

    function retrieve() public view returns (uint256) {
        return _num;
    }
}
