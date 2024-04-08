// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.11;


import { IMarketplace } from "../interfaces/marketplace/IMarketplace.sol";
import "../lib/CurrencyTransferLib.sol";
import "../lib/FeeType.sol";


contract Marketplace {
    
    event __cubist_event_Marketplace_buy(uint256 _listingId, address _buyFor, uint256 _quantity, address _currency, uint256 _totalPrice);
	
    function buy(uint256 _listingId, address _buyFor, uint256 _quantity, address _currency, uint256 _totalPrice) public {
        emit __cubist_event_Marketplace_buy(_listingId, _buyFor, _quantity, _currency, _totalPrice);
    }

}

