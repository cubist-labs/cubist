// SPDX-License-Identifier: MIT
pragma solidity ^0.8.9;

import "../foo/Foo.sol";
import "../Common.sol";

contract Bar {
    Foo private _foo;
    uint256 _num;

    constructor(Foo foo) {
        _foo = foo;
    }

    function call_foo(uint256 num) public {
        _foo.store(num);
    }

    function store(uint256 num) public {
        _num = num;
    }

    function retrieve() public view returns (uint256) {
        return _num;
    }
}
