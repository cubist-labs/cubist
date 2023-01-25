{%- import "macros.tpl" as macros -%}
{{macros::axelar_header(file=file)}}

{%- for contract in file.interfaces %}
contract {{contract.contract}}Interface is AxelarExecutable {
    IAxelarGasService public immutable gasReceiver;
    {{contract.contract}} contractObject;

    constructor(
        address gateway_,
        address gasReceiver_,
        address contractAddress
    ) AxelarExecutable(gateway_) {
        gasReceiver = IAxelarGasService(gasReceiver_);
        contractObject = {{contract.contract}}(contractAddress);
    }

    function _execute(
        string calldata,
        string calldata,
        bytes calldata payload_
    ) internal override {
        (bool success,) = address(contractObject).call(payload_);
        require(success);
    }
}
{% endfor %}
