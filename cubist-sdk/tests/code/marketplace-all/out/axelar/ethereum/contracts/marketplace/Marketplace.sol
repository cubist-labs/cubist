// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.16;
import {AxelarExecutable} from "@axelar-network/axelar-gmp-sdk-solidity/contracts/executables/AxelarExecutable.sol";
import {IAxelarGateway} from "@axelar-network/axelar-gmp-sdk-solidity/contracts/interfaces/IAxelarGateway.sol";
import {IAxelarGasService} from "@axelar-network/axelar-gmp-sdk-solidity/contracts/interfaces/IAxelarGasService.sol";
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
   

contract Marketplace is AxelarExecutable {
    
    IAxelarGasService public immutable gasReceiver;
    
    // The receiver interface address
    string interfaceAddress;

    constructor(
        address gateway_,
        address gasReceiver_,
        string memory interfaceAddress_
    ) AxelarExecutable(gateway_) {
        gasReceiver = IAxelarGasService(gasReceiver_);
        interfaceAddress = interfaceAddress_;
    }

    
    function initialize(address _defaultAdmin, string _contractURI, address[] _trustedForwarders, address _platformFeeRecipient, uint256 _platformFeeBps) external payable {
      bytes memory payload = abi.encodeWithSignature("initialize(_defaultAdmin, _contractURI, _trustedForwarders, _platformFeeRecipient, _platformFeeBps)", address, string, address[], address, uint256);
      gasReceiver.payNativeGasForContractCall{value: msg.value}(
            address(this),
            "avalanche",
            interfaceAddress,
            payload,
            msg.sender
        );  
    }
    
    function createListing(ListingParameters _params) external payable {
      bytes memory payload = abi.encodeWithSignature("createListing(_params)", ListingParameters);
      gasReceiver.payNativeGasForContractCall{value: msg.value}(
            address(this),
            "avalanche",
            interfaceAddress,
            payload,
            msg.sender
        );  
    }
    
    function updateListing(uint256 _listingId, uint256 _quantityToList, uint256 _reservePricePerToken, uint256 _buyoutPricePerToken, address _currencyToAccept, uint256 _startTime, uint256 _secondsUntilEndTime) external payable {
      bytes memory payload = abi.encodeWithSignature("updateListing(_listingId, _quantityToList, _reservePricePerToken, _buyoutPricePerToken, _currencyToAccept, _startTime, _secondsUntilEndTime)", uint256, uint256, uint256, uint256, address, uint256, uint256);
      gasReceiver.payNativeGasForContractCall{value: msg.value}(
            address(this),
            "avalanche",
            interfaceAddress,
            payload,
            msg.sender
        );  
    }
    
    function cancelDirectListing(uint256 _listingId) external payable {
      bytes memory payload = abi.encodeWithSignature("cancelDirectListing(_listingId)", uint256);
      gasReceiver.payNativeGasForContractCall{value: msg.value}(
            address(this),
            "avalanche",
            interfaceAddress,
            payload,
            msg.sender
        );  
    }
    
    function buy(uint256 _listingId, address _buyFor, uint256 _quantityToBuy, address _currency, uint256 _totalPrice) external payable {
      bytes memory payload = abi.encodeWithSignature("buy(_listingId, _buyFor, _quantityToBuy, _currency, _totalPrice)", uint256, address, uint256, address, uint256);
      gasReceiver.payNativeGasForContractCall{value: msg.value}(
            address(this),
            "avalanche",
            interfaceAddress,
            payload,
            msg.sender
        );  
    }
    
    function acceptOffer(uint256 _listingId, address _offeror, address _currency, uint256 _pricePerToken) external payable {
      bytes memory payload = abi.encodeWithSignature("acceptOffer(_listingId, _offeror, _currency, _pricePerToken)", uint256, address, address, uint256);
      gasReceiver.payNativeGasForContractCall{value: msg.value}(
            address(this),
            "avalanche",
            interfaceAddress,
            payload,
            msg.sender
        );  
    }
    
    function offer(uint256 _listingId, uint256 _quantityWanted, address _currency, uint256 _pricePerToken, uint256 _expirationTimestamp) external payable {
      bytes memory payload = abi.encodeWithSignature("offer(_listingId, _quantityWanted, _currency, _pricePerToken, _expirationTimestamp)", uint256, uint256, address, uint256, uint256);
      gasReceiver.payNativeGasForContractCall{value: msg.value}(
            address(this),
            "avalanche",
            interfaceAddress,
            payload,
            msg.sender
        );  
    }
    
    function closeAuction(uint256 _listingId, address _closeFor) external payable {
      bytes memory payload = abi.encodeWithSignature("closeAuction(_listingId, _closeFor)", uint256, address);
      gasReceiver.payNativeGasForContractCall{value: msg.value}(
            address(this),
            "avalanche",
            interfaceAddress,
            payload,
            msg.sender
        );  
    }
    
    function setPlatformFeeInfo(address _platformFeeRecipient, uint256 _platformFeeBps) external payable {
      bytes memory payload = abi.encodeWithSignature("setPlatformFeeInfo(_platformFeeRecipient, _platformFeeBps)", address, uint256);
      gasReceiver.payNativeGasForContractCall{value: msg.value}(
            address(this),
            "avalanche",
            interfaceAddress,
            payload,
            msg.sender
        );  
    }
    
    function setAuctionBuffers(uint256 _timeBuffer, uint256 _bidBufferBps) external payable {
      bytes memory payload = abi.encodeWithSignature("setAuctionBuffers(_timeBuffer, _bidBufferBps)", uint256, uint256);
      gasReceiver.payNativeGasForContractCall{value: msg.value}(
            address(this),
            "avalanche",
            interfaceAddress,
            payload,
            msg.sender
        );  
    }
    
    function setContractURI(string _uri) external payable {
      bytes memory payload = abi.encodeWithSignature("setContractURI(_uri)", string);
      gasReceiver.payNativeGasForContractCall{value: msg.value}(
            address(this),
            "avalanche",
            interfaceAddress,
            payload,
            msg.sender
        );  
    }
    
}

