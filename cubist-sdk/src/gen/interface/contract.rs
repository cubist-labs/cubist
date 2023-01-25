/// An interface for exposing a single contract cross-chain
use crate::gen::common::{InterfaceGenError, Result};
use crate::gen::interface::config::InterfaceConfig;
use cubist_config::{ContractName, FunctionName, ParamName};
use serde::{Serialize, Serializer};
use solang_parser::pt;
use solang_parser::pt::Docable;
use std::fmt;

/// A cross-chain interface for a contract
/// This is an interface that exposes all exposable definitions
/// in the contract (e.g., functions or enums) for deployment
/// on other chains.
#[derive(Debug, Serialize)]
pub struct ContractInterface {
    /// The contract that this interface exposes
    contract: ContractName,

    /// The functions that bridge from one chain to another
    functions: Vec<Function>,

    /// The code that must be copied from the original contract to the interface
    /// This includes struct definitions, enums, newtypes, etc
    forwarded_code: Vec<Code>,
}

#[derive(Debug, Serialize)]
pub struct Function {
    name: FunctionName,
    params: Vec<Param>,
    attrs: Vec<String>,
    is_payable: bool,
}

#[derive(Debug, Serialize)]
pub struct Param {
    name: ParamName,
    ty: Expression,
}

#[derive(Debug)]
pub struct Code(pt::ContractPart);

impl Serialize for Code {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        // XXX: hack. I'm so sorry. Tera sort of abuses serialization...
        let str = self.to_string();
        serializer.serialize_str(&str)
    }
}

impl fmt::Display for Code {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.0.display())
    }
}

#[derive(Debug)]
pub struct Expression(pt::Expression);

impl Serialize for Expression {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let str = self.to_string();
        serializer.serialize_str(&str)
    }
}

impl fmt::Display for Expression {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.0.display())
    }
}

impl ContractInterface {
    /// Create a new cross-chain interface for {contract}, using information about which
    /// functions to create cross-chain interfaces for from {config}
    pub fn new(config: &InterfaceConfig, contract: &pt::ContractDefinition) -> Result<Self> {
        let mut code = Vec::new();
        let mut functions = Vec::<Function>::new();
        // Check which functions we've seen
        // (for useful error messages about generated getters)
        let mut seen = Vec::new();
        let name = &contract.name.name;
        for part in &contract.parts {
            // MLFB: Leaving this match very verbose for now
            match part {
                // Struct, enum, and type definitions are always legal cross-chain
                pt::ContractPart::StructDefinition(..) => code.push(Code(part.clone())),
                pt::ContractPart::EnumDefinition(..) => code.push(Code(part.clone())),
                pt::ContractPart::TypeDefinition(..) => code.push(Code(part.clone())),
                pt::ContractPart::EventDefinition(ev) if event_is_legal(ev) => {
                    code.push(Code(part.clone()))
                }
                pt::ContractPart::ErrorDefinition(er) if error_is_legal(er) => {
                    code.push(Code(part.clone()))
                }
                // We don't allow functions with return values to be cross-chain,
                // so we certainly don't allow implicit getters for public
                // variables. To disallow such getters, we just don't include
                // any contract variables in our cross-chain interface
                pt::ContractPart::VariableDefinition(..) => (),
                pt::ContractPart::FunctionDefinition(fd) => {
                    if function_is_legal(fd) {
                        // All legal functions have a name
                        let function_name = &fd.name.as_ref().unwrap().name;
                        seen.push(function_name);
                        // Should we be generating an interface for this function?
                        if config.gen_function(name, function_name) {
                            // All legal functions have types
                            let params = fd
                                .params
                                .iter()
                                .map(|p| {
                                    let param = p.1.as_ref().unwrap();
                                    Param {
                                        // All legal functions have named arguments
                                        name: param.name.as_ref().unwrap().name.to_string(),
                                        ty: Expression(param.ty.clone()),
                                    }
                                })
                                .collect::<Vec<Param>>();
                            let attrs: Vec<String> =
                                fd.attributes.iter().map(|a| a.display()).collect();
                            functions.push(Function {
                                name: function_name.to_string(),
                                params,
                                is_payable: attrs.iter().any(|s| s == "payable"),
                                attrs,
                            });
                        }
                    } else {
                        // Is this a function we should be generating an interface for,
                        // but actually can't? (e.g., because it is private)
                        // This *doesn't matter* in the case that we're just
                        // exposing all exposable functions---in that case, anything
                        // non-exposable is just ignored.
                        if !config.expose_all() && fd.name.is_some() {
                            let function_name = &fd.name.as_ref().unwrap().name;
                            seen.push(function_name);
                            if config.gen_function(name, function_name) {
                                return Err(InterfaceGenError::GenerateInterfaceError(
                                    function_name.clone(),
                                ));
                            }
                        }
                    }
                }
                // We don't need to copy stray semicolons
                pt::ContractPart::StraySemicolon(..) => (),
                // Forward "using" in case one of the types the interface
                // relies on the "using" shorthand
                pt::ContractPart::Using(..) => code.push(Code(part.clone())),
                // Don't copy anything banned
                _ => (),
            }
        }
        // Were we supposed to make an interface for something we didn't see?
        // This can happen e.g., in the case of implicit getters
        if let Some(missing) = config.missed_function(name, &seen) {
            return Err(InterfaceGenError::MissingFunction(missing));
        }

        Ok(ContractInterface {
            contract: name.to_string(),
            functions,
            forwarded_code: code,
        })
    }

    pub fn get_contract_name(&self) -> &String {
        &self.contract
    }

    pub fn get_functions(&self) -> &Vec<Function> {
        &self.functions
    }
}

impl Function {
    pub fn name(&self) -> &FunctionName {
        &self.name
    }
}

/// Can {ev} be exposed cross-chain?
fn event_is_legal(_ev: &pt::EventDefinition) -> bool {
    true
}

/// Can {er} be exposed cross-chain?
fn error_is_legal(_er: &pt::ErrorDefinition) -> bool {
    true
}

/// Can {fd} be exposed cross-chain?
fn function_is_legal(fd: &pt::FunctionDefinition) -> bool {
    // No return values
    if !fd.returns.is_empty()
	// No constructors, modifiers, fallbacks, or receivers
        | (fd.ty != pt::FunctionTy::Function)
        // No annonymous functions
	// prior check *should* get those, but being safe
        | fd.name.is_none()
	// No private or internal functions 
        | fd.attributes.iter().any(|attr| {
            matches!(
                attr,
                pt::FunctionAttribute::Visibility(pt::Visibility::Internal(..))
                    | pt::FunctionAttribute::Visibility(pt::Visibility::Private(..))
            )
        })
	// No annonymous parameters 
	| fd.params.iter().any(|(_, param)|
			       param.is_none() || param.as_ref().unwrap().name.is_none())
    {
        return false;
    }
    true
}
