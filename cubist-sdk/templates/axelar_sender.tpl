{%- import "macros.tpl" as macros -%}
{{macros::axelar_header(file=file)}}

{%- for contract in file.interfaces %}
contract {{contract.contract}} is AxelarExecutable {
    {% for forward in contract.forwarded_code -%}
    {{forward}}
    {% endfor %}
    IAxelarGasService public immutable _gasReceiver;

    // The receiver interface address
    string _interfaceAddress;

    {{macros::access_control()}}

    constructor(
        address gateway,
        address gasReceiver,
        string memory interfaceAddress
    ) AxelarExecutable(gateway) {
        _gasReceiver = IAxelarGasService(gasReceiver);
        _interfaceAddress = interfaceAddress;
        _owner = msg.sender;
    }

    {% for function in contract.functions %}
    function {{function.name}}({{macros::params(args=function.params)}}) external payable onlyCaller {
      bytes memory payload = abi.encodeWithSignature("{{function.name}}({{macros::arg_types(args=function.params)}})"{{macros::comma(list=function.params)}}{{macros::arg_names(args=function.params)}});
      _gasReceiver.payNativeGasForContractCall{value: msg.value}(
            address(this),
            "{{axl_dest_chain}}",
            _interfaceAddress,
            payload,
            msg.sender
        );
        gateway.callContract("{{axl_dest_chain}}", _interfaceAddress, payload);
    }
    {% endfor %}
}
{% endfor %}
