// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.11;


import "@openzeppelin/contracts-upgradeable/token/ERC20/IERC20Upgradeable.sol";
import "@openzeppelin/contracts-upgradeable/token/ERC1155/IERC1155Upgradeable.sol";
import "@openzeppelin/contracts-upgradeable/token/ERC721/IERC721Upgradeable.sol";
import "@openzeppelin/contracts-upgradeable/token/ERC1155/IERC1155ReceiverUpgradeable.sol";
import "@openzeppelin/contracts-upgradeable/token/ERC721/IERC721ReceiverUpgradeable.sol";
import "@openzeppelin/contracts-upgradeable/utils/introspection/IERC165Upgradeable.sol";
import "@openzeppelin/contracts-upgradeable/interfaces/IERC2981Upgradeable.sol";
import "@openzeppelin/contracts-upgradeable/access/AccessControlEnumerableUpgradeable.sol";
import "@openzeppelin/contracts-upgradeable/security/ReentrancyGuardUpgradeable.sol";
import "@openzeppelin/contracts-upgradeable/utils/MulticallUpgradeable.sol";
import { IMarketplace } from "../interfaces/marketplace/IMarketplace.sol";
import "../openzeppelin-presets/metatx/ERC2771ContextUpgradeable.sol";
import "../lib/CurrencyTransferLib.sol";
import "../lib/FeeType.sol";


contract Marketplace {
    
    event __cubist_event_Marketplace_initialize(address _defaultAdmin, string _contractURI, address[] _trustedForwarders, address _platformFeeRecipient, uint256 _platformFeeBps);
	
    function initialize(address _defaultAdmin, string _contractURI, address[] _trustedForwarders, address _platformFeeRecipient, uint256 _platformFeeBps) public {
        emit __cubist_event_Marketplace_initialize(_defaultAdmin, _contractURI, _trustedForwarders, _platformFeeRecipient, _platformFeeBps);
    }
event __cubist_event_Marketplace_createListing(ListingParameters _params);
	
    function createListing(ListingParameters _params) public {
        emit __cubist_event_Marketplace_createListing(_params);
    }
event __cubist_event_Marketplace_updateListing(uint256 _listingId, uint256 _quantityToList, uint256 _reservePricePerToken, uint256 _buyoutPricePerToken, address _currencyToAccept, uint256 _startTime, uint256 _secondsUntilEndTime);
	
    function updateListing(uint256 _listingId, uint256 _quantityToList, uint256 _reservePricePerToken, uint256 _buyoutPricePerToken, address _currencyToAccept, uint256 _startTime, uint256 _secondsUntilEndTime) public {
        emit __cubist_event_Marketplace_updateListing(_listingId, _quantityToList, _reservePricePerToken, _buyoutPricePerToken, _currencyToAccept, _startTime, _secondsUntilEndTime);
    }
event __cubist_event_Marketplace_cancelDirectListing(uint256 _listingId);
	
    function cancelDirectListing(uint256 _listingId) public {
        emit __cubist_event_Marketplace_cancelDirectListing(_listingId);
    }
event __cubist_event_Marketplace_buy(uint256 _listingId, address _buyFor, uint256 _quantityToBuy, address _currency, uint256 _totalPrice);
	
    function buy(uint256 _listingId, address _buyFor, uint256 _quantityToBuy, address _currency, uint256 _totalPrice) public {
        emit __cubist_event_Marketplace_buy(_listingId, _buyFor, _quantityToBuy, _currency, _totalPrice);
    }
event __cubist_event_Marketplace_acceptOffer(uint256 _listingId, address _offeror, address _currency, uint256 _pricePerToken);
	
    function acceptOffer(uint256 _listingId, address _offeror, address _currency, uint256 _pricePerToken) public {
        emit __cubist_event_Marketplace_acceptOffer(_listingId, _offeror, _currency, _pricePerToken);
    }
event __cubist_event_Marketplace_offer(uint256 _listingId, uint256 _quantityWanted, address _currency, uint256 _pricePerToken, uint256 _expirationTimestamp);
	
    function offer(uint256 _listingId, uint256 _quantityWanted, address _currency, uint256 _pricePerToken, uint256 _expirationTimestamp) public {
        emit __cubist_event_Marketplace_offer(_listingId, _quantityWanted, _currency, _pricePerToken, _expirationTimestamp);
    }
event __cubist_event_Marketplace_closeAuction(uint256 _listingId, address _closeFor);
	
    function closeAuction(uint256 _listingId, address _closeFor) public {
        emit __cubist_event_Marketplace_closeAuction(_listingId, _closeFor);
    }
event __cubist_event_Marketplace_setPlatformFeeInfo(address _platformFeeRecipient, uint256 _platformFeeBps);
	
    function setPlatformFeeInfo(address _platformFeeRecipient, uint256 _platformFeeBps) public {
        emit __cubist_event_Marketplace_setPlatformFeeInfo(_platformFeeRecipient, _platformFeeBps);
    }
event __cubist_event_Marketplace_setAuctionBuffers(uint256 _timeBuffer, uint256 _bidBufferBps);
	
    function setAuctionBuffers(uint256 _timeBuffer, uint256 _bidBufferBps) public {
        emit __cubist_event_Marketplace_setAuctionBuffers(_timeBuffer, _bidBufferBps);
    }
event __cubist_event_Marketplace_setContractURI(string _uri);
	
    function setContractURI(string _uri) public {
        emit __cubist_event_Marketplace_setContractURI(_uri);
    }

}

