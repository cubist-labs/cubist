{%- macro params(args) -%}{%- for arg in args -%}{{arg.ty}} {{arg.name}}{%- set len = args | length -%}{%- if loop.index < len -%}, {% endif %}{% endfor %}{% endmacro params %}

{%- macro arg_names(args) -%}{%- for arg in args -%}{{arg.name}}{%- set len = args | length -%}{%- if loop.index < len -%}, {% endif %}{% endfor %}{% endmacro args %}

{%- macro arg_types(args) -%}{%- for arg in args -%}{{arg.ty}}{%- set len = args | length -%}{%- if loop.index < len -%}, {% endif %}{% endfor %}{% endmacro args %}

{%- macro license(license) -%}{%- if license -%}// SPDX-License-Identifier: {{license}}{% endif %}{% endmacro %}

{%- macro payable(is_payable) -%}{%- if is_payable -%}payable {% endif %}{% endmacro %}

{%- macro bridges(contract) -%}{%- for function in contract.functions -%}
"{{function.name}}": "__cubist_event_{{contract.contract}}_{{function.name}}"{%- set len = contract.functions | length -%}{%- if loop.index < len -%},{% endif %}{% endfor %}{% endmacro args %}

{%- macro comma(list) -%}{%- set len = list | length -%}{%- if len > 0 -%}, {% endif %}{% endmacro %}

{%- macro axelar_header(file) -%}
{%- if file.license -%}// SPDX-License-Identifier: {{file.license}}{% endif %}
pragma solidity ^0.8.16;
import {AxelarExecutable} from "{{axelar_package}}/contracts/executables/AxelarExecutable.sol";
import {IAxelarGateway} from "{{axelar_package}}/contracts/interfaces/IAxelarGateway.sol";
import {IAxelarGasService} from "{{axelar_package}}/contracts/interfaces/IAxelarGasService.sol";
{% for import in file.imports -%}
{{import}}
{% endfor %}   
{% endmacro %}