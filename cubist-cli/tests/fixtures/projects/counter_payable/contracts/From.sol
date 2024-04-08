// SPDX-License-Identifier: MIT
pragma solidity ^0.8.9;

import "./To.sol";

contract From {
    To private _to;
    uint256 _num;

    constructor(To to) {
        _to = to;
    }

    function store(uint256 num) public payable {
        _num = num;
        _to.store{value: msg.value}(num);
    }

    function retrieve() public view returns (uint256) {
        return _num;
    }
}
