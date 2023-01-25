contract EthStorage {

    uint256 number;

    constructor (uint256 num) {
      number = num;
    }

    function store(uint256 num) public {
      number = num;
    }

    function retrieve() public view returns (uint256){
        return number;
    }
}