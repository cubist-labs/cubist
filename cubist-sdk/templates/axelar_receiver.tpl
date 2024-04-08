{%- import "macros.tpl" as macros -%}
{{macros::axelar_header(file=file)}}

import "{{target_receiver_import_path}}";

{%- for contract in file.interfaces %}
contract {{contract.contract}}Interface is AxelarExecutable {
    {{contract.contract}} _contractObject;

    {{macros::access_control()}}

    constructor(address gateway) AxelarExecutable(gateway) {
        _owner = msg.sender;
    }

    function {{AXELAR_SET_TARGET_ADDR_METHOD_NAME}}(address contractAddress) public onlyOwner {
        _contractObject = {{contract.contract}}(contractAddress);
    }

    function _execute(
        string calldata,
        string calldata,
        bytes calldata payload
    ) internal override {
        (bool success,) = address(_contractObject).call(payload);
        require(success, "Calling target contract failed");
    }
}
{% endfor %}
