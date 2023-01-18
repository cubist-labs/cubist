// SPDX-License-Identifier: MIT
pragma solidity ^0.8.7;
import './Used.sol';

contract User {
    Used used;

    function addBookRemote(Used.Book memory book) public {
        Used.Book memory bellJar = Used.Book("The Bell Jar", "Sylvia Plath", 244);
        used.addBook(book);
    }
}