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
import { IMarketplace } from "./marketplace/contracts/interfaces/marketplace/IMarketplace.sol";
import "./marketplace/contracts/openzeppelin-presets/metatx/ERC2771ContextUpgradeable.sol";
import "./marketplace/contracts/lib/CurrencyTransferLib.sol";
import "./marketplace/contracts/lib/FeeType.sol";


contract Marketplace {
    
    event __cubist_event_Marketplace_buy(uint256 _listingId, address _buyFor, uint256 _quantityToBuy, address _currency, uint256 _totalPrice);
	
    function buy(uint256 _listingId, address _buyFor, uint256 _quantityToBuy, address _currency, uint256 _totalPrice) public {
        emit __cubist_event_Marketplace_buy(_listingId, _buyFor, _quantityToBuy, _currency, _totalPrice);
    }
event __cubist_event_Marketplace_offer(uint256 _listingId, uint256 _quantityWanted, address _currency, uint256 _pricePerToken, uint256 _expirationTimestamp);
	
    function offer(uint256 _listingId, uint256 _quantityWanted, address _currency, uint256 _pricePerToken, uint256 _expirationTimestamp) public {
        emit __cubist_event_Marketplace_offer(_listingId, _quantityWanted, _currency, _pricePerToken, _expirationTimestamp);
    }

}

