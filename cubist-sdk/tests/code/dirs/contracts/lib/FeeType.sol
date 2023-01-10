// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.11;

// Thanks to: https://raw.githubusercontent.com/thirdweb-dev/contracts/main/contracts/lib/FeeType.sol

library FeeType {
    uint256 internal constant PRIMARY_SALE = 0;
    uint256 internal constant MARKET_SALE = 1;
    uint256 internal constant SPLIT = 2;
}
