{%- import "macros.tpl" as macros -%}
{{macros::axelar_header(file=file)}}

{%- for contract in file.interfaces %}
contract {{contract.contract}} is AxelarExecutable {
    {% for forward in contract.forwarded_code -%}
    {{forward}}
    {% endfor %}
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

    {% for function in contract.functions %}
    function {{function.name}}({{macros::params(args=function.params)}}) external payable {
      bytes memory payload = abi.encodeWithSignature("{{function.name}}({{macros::args(args=function.params)}})"{{macros::comma(list=function.params)}}{{macros::tys(args=function.params)}});
      gasReceiver.payNativeGasForContractCall{value: msg.value}(
            address(this),
            "{{file.receiver_target}}",
            interfaceAddress,
            payload,
            msg.sender
        );  
    }
    {% endfor %}
}
{% endfor %}
