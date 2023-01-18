// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.11;
import 'marketplace/Marketplace.sol';

contract MarketplaceUser {
    Marketplace marketplace;

    function buy(
        uint256 _listingId,
        address _buyFor,
        uint256 _quantity,
        address _currency,
        uint256 _totalPrice
    ) public payable {
        marketplace.buy(_listingId, _buyFor, _quantity, _currency, _totalPrice);
    }
}