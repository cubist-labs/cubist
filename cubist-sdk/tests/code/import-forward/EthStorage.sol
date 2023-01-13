// SPDX-License-Identifier: MIT
pragma solidity ^0.8.17;

import "@openzeppelin/contracts/token/ERC721/ERC721.sol";
import "./Util.sol" as Util;

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

contract OtherContract {

}