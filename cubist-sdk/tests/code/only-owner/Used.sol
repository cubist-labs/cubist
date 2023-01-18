//SPDX-License-Identifier: MIT
// Thanks to: https://github.com/ava-labs/avalanche-smart-contract-quickstart/blob/main/contracts/ExampleERC20.sol
pragma solidity >=0.6.2;

import "@openzeppelin/contracts/token/ERC20/ERC20.sol";
import "@openzeppelin/contracts/access/Ownable.sol";

contract Used is ERC20, Ownable {
  string private TOKEN_NAME = "Example ERC20 Token";
  string private TOKEN_SYMBOL = "XMPL";

  uint256 private constant TOTAL_SUPPLY = 123456789;

  constructor()ERC20(TOKEN_NAME, TOKEN_SYMBOL) {
    _mint(msg.sender, TOTAL_SUPPLY);
  }

  function mint(address to, uint256 amount) public onlyOwner {
    _mint(to, amount);
  }

  function burn(address from, uint256 amount) public onlyOwner {
    _burn(from, amount);
  }
}
