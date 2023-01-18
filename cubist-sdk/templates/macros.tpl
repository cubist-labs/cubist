{%- macro params(args) -%}{%- for arg in args -%}{{arg.ty}} {{arg.name}}{%- set len = args | length -%}{%- if loop.index < len -%}, {% endif %}{% endfor %}{% endmacro params %}

{%- macro args(args) -%}{%- for arg in args -%}{{arg.name}}{%- set len = args | length -%}{%- if loop.index < len -%}, {% endif %}{% endfor %}{% endmacro args %}

{%- macro tys(args) -%}{%- for arg in args -%}{{arg.ty}}{%- set len = args | length -%}{%- if loop.index < len -%}, {% endif %}{% endfor %}{% endmacro args %}

{%- macro license(license) -%}{%- if license -%}// SPDX-License-Identifier: {{license}}{% endif %}{% endmacro %}

{%- macro bridges(contract) -%}{%- for function in contract.functions -%}
"{{function.name}}": "__cubist_event_{{contract.contract}}_{{function.name}}"{%- set len = contract.functions | length -%}{%- if loop.index < len -%},{% endif %}{% endfor %}{% endmacro args %}

{%- macro comma(list) -%}{%- set len = list | length -%}{%- if len > 0 -%}, {% endif %}{% endmacro %}

{%- macro axelar_header(file) -%}
{%- if file.license -%}// SPDX-License-Identifier: {{file.license}}{% endif %}
pragma solidity ^0.8.16;
import {AxelarExecutable} from "@axelar-network/axelar-gmp-sdk-solidity/contracts/executables/AxelarExecutable.sol";
import {IAxelarGateway} from "@axelar-network/axelar-gmp-sdk-solidity/contracts/interfaces/IAxelarGateway.sol";
import {IAxelarGasService} from "@axelar-network/axelar-gmp-sdk-solidity/contracts/interfaces/IAxelarGasService.sol";
{% for import in file.imports -%}
{{import}}
{% endfor %}   
{% endmacro %}