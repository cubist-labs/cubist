// SPDX-License-Identifier: MIT
pragma solidity ^0.8.7;

contract Used {
    struct Book {
        string name;
        string author;
        uint256 numPages;
    }

    Book[] myBooks;

    function addBook(Book memory book) public {
        myBooks.push(book);
    }
}