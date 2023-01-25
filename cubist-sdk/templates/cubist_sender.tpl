{%- import "macros.tpl" as macros -%}
{{macros::license(license=file.license)}}
{% for pragma in file.pragmas -%}
{{pragma}}
{% endfor %}
{% for import in file.imports -%}
{{import}}
{% endfor %}

{% for contract in file.interfaces %}
contract {{contract.contract}} {
    {% for forward in contract.forwarded_code -%}
    {{forward}}
    {% endfor %}
    {% for function in contract.functions -%}
    event __cubist_event_{{contract.contract}}_{{function.name}}({{macros::params(args=function.params)}});

    address private _owner;
    mapping(address => bool) private _callers;

    function _isOwner(address account) private view returns (bool) {
        return _owner == account;
    }

    function _isCaller(address account) private view returns (bool) {
        return _callers[account];
    }

    modifier onlyOwner() {
        require(_isOwner(msg.sender), "Cubist: sender is not the owner");
        _;
    }

    modifier onlyCaller() {
        require(_isCaller(msg.sender), "Cubist: sender is not a caller");
        _;
    }

    constructor() {
        _owner = msg.sender;
    }

    function {{APPROVE_CALLER_METHOD_NAME}}(address account) public onlyOwner {
        _callers[account] = true;
    }
	
    function {{function.name}}({{macros::params(args=function.params)}}) public onlyCaller {{macros::payable(is_payable=function.is_payable)}}{
        emit __cubist_event_{{contract.contract}}_{{function.name}}({{macros::arg_names(args=function.params)}});
    }
{% endfor %}
}
{% endfor %}
