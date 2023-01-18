// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.11;

// Copying internal imports from: https://github.com/thirdweb-dev/contracts/blob/main/contracts/marketplace/Marketplace.sol

import { IMarketplace } from "../interfaces/marketplace/IMarketplace.sol";
import "../lib/CurrencyTransferLib.sol";
import "../lib/FeeType.sol";

contract Marketplace is IMarketplace {
    uint256 price;
    uint256 id;
    address boughtFor;
    address currency;

    function buy(
        uint256 _listingId,
        address _buyFor,
        uint256 _quantity,
        address _currency,
        uint256 _totalPrice
    ) public payable {
        id = _listingId;
	boughtFor = _buyFor;
        price = _totalPrice * _quantity;
	currency = _currency;
    }

    function acceptOffer(
        uint256 _listingId,
        address _offeror,
        address _currency,
        uint256 _totalPrice
    ) public {
    }

    function updateListing(
        uint256 _listingId,
        uint256 _quantityToList,
        uint256 _reservePricePerToken,
        uint256 _buyoutPricePerToken,
        address _currencyToAccept,
        uint256 _startTime,
        uint256 _secondsUntilEndTime
    ) public {
    }

    function createListing(ListingParameters memory _params) public {
    }

    function offer(
        uint256 _listingId,
        uint256 _quantityWanted,
        address _currency,
        uint256 _pricePerToken,
        uint256 _expirationTimestamp
    ) public payable {
    }

    function closeAuction(uint256 _listingId, address _closeFor) public {
    }

    function cancelDirectListing(uint256 _listingId) public {
    }

    function contractType() external pure returns (bytes32) {
        return 0;
    }

    function contractURI() external pure returns (string memory) {
        return "uri";
    }

    function contractVersion() external pure returns (uint8) {
        return 0;
    }

    function getPlatformFeeInfo() external view returns (address, uint16) {
        return (address(this), 0);
    }

    function setContractURI(string calldata _uri) public pure {
    }

    function setPlatformFeeInfo(address _platformFeeRecipient, uint256 _platformFeeBps) public pure {
    
    }
}
