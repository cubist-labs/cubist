// SPDX-License-Identifier: MIT

pragma solidity ^0.8.7;
import './DynNFT.sol';

contract Grower {
    DynNFT nft;	

    // grow the flower
    function grow(uint256 id) public {
        nft.growFlower(id);
    }
}
