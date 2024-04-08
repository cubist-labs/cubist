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

    {{macros::access_control()}}

    constructor() {
        _owner = msg.sender;
    }

    {% for function in contract.functions -%}
    event __cubist_event_{{contract.contract}}_{{function.name}}({{macros::params(args=function.params)}});

    function {{function.name}}({{macros::params(args=function.params)}}) public onlyCaller {{macros::payable(is_payable=function.is_payable)}}{
        emit __cubist_event_{{contract.contract}}_{{function.name}}({{macros::arg_names(args=function.params)}});
    }
    {% endfor %}
}
{% endfor %}
