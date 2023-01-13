//! AST walker for Solang's parse tree
use crate::gen::common::Result;
use solang_parser::pt;

pub struct DefaultVisitor {}

impl Visitor for DefaultVisitor {}

pub trait Visitor {
    fn visit_source_unit(&mut self, su: &pt::SourceUnit) -> Result<()> {
        walk_source_unit(self, su)
    }

    fn visit_source_unit_part(&mut self, su: &pt::SourceUnitPart) -> Result<()> {
        walk_source_unit_part(self, su)
    }

    fn visit_contract_definition(&mut self, cd: &pt::ContractDefinition) -> Result<()> {
        walk_contract_definition(self, cd)
    }

    fn visit_struct_definition(&mut self, sd: &pt::StructDefinition) -> Result<()> {
        walk_struct_definition(self, sd)
    }

    fn visit_event_definition(&mut self, ed: &pt::EventDefinition) -> Result<()> {
        walk_event_definition(self, ed)
    }

    fn visit_error_definition(&mut self, ed: &pt::ErrorDefinition) -> Result<()> {
        walk_error_definition(self, ed)
    }

    fn visit_function_definition(&mut self, def: &pt::FunctionDefinition) -> Result<()> {
        walk_function_definition(self, def)
    }

    fn visit_variable_declaration(&mut self, vd: &pt::VariableDeclaration) -> Result<()> {
        walk_variable_declaration(self, vd)
    }

    fn visit_type_definition(&mut self, def: &pt::TypeDefinition) -> Result<()> {
        walk_type_definition(self, def)
    }

    fn visit_using(&mut self, u: &pt::Using) -> Result<()> {
        walk_using(self, u)
    }

    fn visit_contract_base(&mut self, base: &pt::Base) -> Result<()> {
        walk_contract_base(self, base)
    }

    fn visit_contract_part(&mut self, cp: &pt::ContractPart) -> Result<()> {
        walk_contract_part(self, cp)
    }

    fn visit_statement(&mut self, stmt: &pt::Statement) -> Result<()> {
        walk_statement(self, stmt)
    }

    fn visit_catchclause(&mut self, cc: &pt::CatchClause) -> Result<()> {
        walk_catchclause(self, cc)
    }

    fn visit_expression(&mut self, expr: &pt::Expression) -> Result<()> {
        walk_expression(self, expr)
    }

    fn visit_variable_definition(&mut self, var: &pt::VariableDefinition) -> Result<()> {
        walk_variable_definition(self, var)
    }

    fn visit_parameter(&mut self, p: &pt::Parameter) -> Result<()> {
        walk_parameter(self, p)
    }

    fn visit_parameterlist(&mut self, pl: &pt::ParameterList) -> Result<()> {
        walk_parameterlist(self, pl)
    }

    // Solang often uses raw expressions as types.
    // This visitor just makes it easier to walk those raw expressions as if they're types
    fn visit_type_expression(&mut self, expr: &pt::Expression) -> Result<()> {
        walk_expression(self, expr)
    }

    fn visit_type(&mut self, ty: &pt::Type) -> Result<()> {
        walk_type(self, ty)
    }
}

// SourceUnit(Vec<SourceUnitPart>)
pub fn walk_source_unit<V: Visitor + ?Sized>(v: &mut V, su: &pt::SourceUnit) -> Result<()> {
    su.0.iter()
        .try_for_each(|part| v.visit_source_unit_part(part))
}

// Documented in match
pub fn walk_source_unit_part<V: Visitor + ?Sized>(
    v: &mut V,
    su: &pt::SourceUnitPart,
) -> Result<()> {
    match su {
        pt::SourceUnitPart::ContractDefinition(cd) => v.visit_contract_definition(cd),
        // PragmaDirective(Loc, Identifier, StringLiteral)
        pt::SourceUnitPart::PragmaDirective(..) => Ok(()),
        // Import
        //   Plain(StringLiteral, Loc),
        //   GlobalSymbol(StringLiteral, Identifier, Loc)
        //   Rename(StringLiteral, Vec<(Identifier, Option<Identifier>)>, Loc)
        pt::SourceUnitPart::ImportDirective(..) => Ok(()),
        // EnumDefinition(Loc, Identifier, Vec<Identifier>)
        pt::SourceUnitPart::EnumDefinition(..) => Ok(()),
        pt::SourceUnitPart::StructDefinition(sd) => v.visit_struct_definition(sd),
        pt::SourceUnitPart::EventDefinition(ed) => v.visit_event_definition(ed),
        pt::SourceUnitPart::ErrorDefinition(ed) => v.visit_error_definition(ed),
        pt::SourceUnitPart::FunctionDefinition(fd) => v.visit_function_definition(fd),
        pt::SourceUnitPart::VariableDefinition(vd) => v.visit_variable_definition(vd),
        pt::SourceUnitPart::TypeDefinition(td) => v.visit_type_definition(td),
        pt::SourceUnitPart::Using(u) => v.visit_using(u),
        // StraySemicolon(Loc)
        pt::SourceUnitPart::StraySemicolon(..) => Ok(()),
    }
}

// ContractDefinition(Loc, ContractTy, Identifier, Vec<Base>, Vec<ContractPart>)
pub fn walk_contract_definition<V: Visitor + ?Sized>(
    v: &mut V,
    cd: &pt::ContractDefinition,
) -> Result<()> {
    cd.base
        .iter()
        .try_for_each(|base| v.visit_contract_base(base))?;
    cd.parts
        .iter()
        .try_for_each(|part| v.visit_contract_part(part))
}

// StructDefinition(Loc, Identifier, Vec<VariableDeclaration>)
pub fn walk_struct_definition<V: Visitor + ?Sized>(
    v: &mut V,
    sd: &pt::StructDefinition,
) -> Result<()> {
    sd.fields
        .iter()
        .try_for_each(|field| v.visit_variable_declaration(field))
}

// EventDefinition(Loc, Identifier, Vec<EventParameter>, bool)
pub fn walk_event_definition<V: Visitor + ?Sized>(
    v: &mut V,
    ed: &pt::EventDefinition,
) -> Result<()> {
    ed.fields
        .iter()
        .try_for_each(|field| v.visit_type_expression(&field.ty))
}

// ErrorDefinition(Loc, Identifier, Vec<ErrorParameter>)
pub fn walk_error_definition<V: Visitor + ?Sized>(
    v: &mut V,
    ed: &pt::ErrorDefinition,
) -> Result<()> {
    ed.fields
        .iter()
        .try_for_each(|field| v.visit_type_expression(&field.ty))
}

// VariableDeclaration(Loc, Expression, Option<StorageLocation>, Identifier)
pub fn walk_variable_declaration<V: Visitor + ?Sized>(
    v: &mut V,
    vd: &pt::VariableDeclaration,
) -> Result<()> {
    v.visit_type_expression(&vd.ty)
}

// Base(Loc, IdentifierPath, Option<Vec<Expression>>)
pub fn walk_contract_base<V: Visitor + ?Sized>(v: &mut V, cb: &pt::Base) -> Result<()> {
    cb.args
        .iter()
        .try_for_each(|a| a.iter().try_for_each(|arg| v.visit_expression(arg)))
}

// Documented in match
pub fn walk_contract_part<V: Visitor + ?Sized>(v: &mut V, cp: &pt::ContractPart) -> Result<()> {
    match cp {
        pt::ContractPart::StructDefinition(sd) => v.visit_struct_definition(sd),
        pt::ContractPart::EventDefinition(ed) => v.visit_event_definition(ed),
        // EnumDefinition(Loc, Identifier, Vec<Identifier>)
        pt::ContractPart::EnumDefinition(..) => Ok(()),
        pt::ContractPart::ErrorDefinition(ed) => v.visit_error_definition(ed),
        pt::ContractPart::VariableDefinition(vd) => v.visit_variable_definition(vd),
        pt::ContractPart::FunctionDefinition(def) => v.visit_function_definition(def),
        pt::ContractPart::TypeDefinition(td) => v.visit_type_definition(td),
        // StraySemicolon(Loc)
        pt::ContractPart::StraySemicolon(..) => Ok(()),
        pt::ContractPart::Using(u) => v.visit_using(u),
    }
}

// FunctionDefinition(Loc, FunctionTy, Option<Identifier>, Loc,
//                    ParameterList, Vec<FunctionAttribute>, Option<Loc>,
//                    ParameterList, Option<Statement>)
pub fn walk_function_definition<V: Visitor + ?Sized>(
    v: &mut V,
    def: &pt::FunctionDefinition,
) -> Result<()> {
    v.visit_parameterlist(&def.params)?;
    v.visit_parameterlist(&def.returns)?;
    def.body.iter().try_for_each(|b| v.visit_statement(b))
}

// Documented in match
pub fn walk_statement<V: Visitor + ?Sized>(v: &mut V, stmt: &pt::Statement) -> Result<()> {
    match stmt {
        pt::Statement::Block { statements, .. } => statements
            .iter()
            .try_for_each(|stmt| v.visit_statement(stmt)),
        // Assembly(Loc, Option<StringLiteral>, Option<Vec<StringLiteral>>, YulBlock)
        // We don't descend into Yul
        pt::Statement::Assembly { .. } => Ok(()),
        pt::Statement::Args(_, args) => args
            .iter()
            .try_for_each(|arg| v.visit_expression(&arg.expr)),
        pt::Statement::If(_, cond, tb, fb) => {
            v.visit_expression(cond)?;
            v.visit_statement(tb)?;
            fb.iter().try_for_each(|b| v.visit_statement(b))
        }
        pt::Statement::While(_, cond, body) => {
            v.visit_expression(cond)?;
            v.visit_statement(body)
        }
        pt::Statement::Expression(_, expr) => v.visit_expression(expr),
        pt::Statement::VariableDefinition(_, vd, expr) => {
            v.visit_variable_declaration(vd)?;
            expr.iter().try_for_each(|e| v.visit_expression(e))
        }
        pt::Statement::For(_, init, incr, bound, body) => {
            init.iter().try_for_each(|init| v.visit_statement(init))?;
            incr.iter().try_for_each(|incr| v.visit_expression(incr))?;
            bound
                .iter()
                .try_for_each(|bound| v.visit_statement(bound))?;
            body.iter().try_for_each(|body| v.visit_statement(body))
        }
        pt::Statement::DoWhile(_, body, cond) => {
            v.visit_statement(body)?;
            v.visit_expression(cond)
        }
        // Continue(Loc)
        pt::Statement::Continue(..) => Ok(()),
        // Break(Loc)
        pt::Statement::Break(..) => Ok(()),
        pt::Statement::Return(_, expr) => expr.iter().try_for_each(|e| v.visit_expression(e)),
        // Revert(Loc, Option<IdentifierPath>, Vec<Expression>)
        pt::Statement::Revert(_, _, exprs) => {
            exprs.iter().try_for_each(|expr| v.visit_expression(expr))
        }
        // RevertNamedArgs(Loc, Option<IdentifierPath>, Vec<NamedArgument>)
        pt::Statement::RevertNamedArgs(_, _, args) => args
            .iter()
            .try_for_each(|arg| v.visit_expression(&arg.expr)),
        // Emit(Loc, Expression)
        pt::Statement::Emit(_, expr) => v.visit_expression(expr),
        // Try(Loc, Expression, Option<(ParameterList, Box<Statement>)>, Vec<CatchClause>)
        pt::Statement::Try(_, expr, stmts, catch) => {
            v.visit_expression(expr)?;
            stmts.iter().try_for_each(|contents| {
                v.visit_parameterlist(&contents.0)?;
                v.visit_statement(&contents.1)
            })?;
            catch
                .iter()
                .try_for_each(|clause| v.visit_catchclause(clause))
        }
    }
}

// Documented in match
pub fn walk_catchclause<V: Visitor + ?Sized>(v: &mut V, cc: &pt::CatchClause) -> Result<()> {
    match cc {
        pt::CatchClause::Simple(_, param, stmt) => {
            if param.is_some() {
                v.visit_parameter(param.as_ref().unwrap())?;
            }
            v.visit_statement(stmt)
        }
        pt::CatchClause::Named(_, _, param, stmt) => {
            v.visit_parameter(param)?;
            v.visit_statement(stmt)
        }
    }
}

// Documented in match
pub fn walk_expression<V: Visitor + ?Sized>(v: &mut V, expr: &pt::Expression) -> Result<()> {
    match expr {
        pt::Expression::PostIncrement(_, expr) => v.visit_expression(expr),
        pt::Expression::PostDecrement(_, expr) => v.visit_expression(expr),
        pt::Expression::New(_, expr) => v.visit_expression(expr),
        pt::Expression::ArraySubscript(_, expr, mexpr) => {
            v.visit_expression(expr)?;
            mexpr.iter().try_for_each(|expr| v.visit_expression(expr))
        }
        pt::Expression::ArraySlice(_, expr, mexpr1, mexpr2) => {
            v.visit_expression(expr)?;
            mexpr1
                .iter()
                .try_for_each(|expr| v.visit_expression(expr))?;
            mexpr2.iter().try_for_each(|expr| v.visit_expression(expr))
        }
        pt::Expression::Parenthesis(_, expr) => v.visit_expression(expr),
        pt::Expression::MemberAccess(_, expr, _) => v.visit_expression(expr),
        pt::Expression::FunctionCall(_, expr, exprs) => {
            v.visit_expression(expr)?;
            exprs.iter().try_for_each(|expr| v.visit_expression(expr))
        }
        pt::Expression::FunctionCallBlock(_, expr, stmt) => {
            v.visit_expression(expr)?;
            v.visit_statement(stmt)
        }
        pt::Expression::NamedFunctionCall(_, expr, args) => {
            v.visit_expression(expr)?;
            args.iter()
                .try_for_each(|arg| v.visit_expression(&arg.expr))
        }
        pt::Expression::Not(_, expr) => v.visit_expression(expr),
        pt::Expression::Complement(_, expr) => v.visit_expression(expr),
        pt::Expression::Delete(_, expr) => v.visit_expression(expr),
        pt::Expression::PreIncrement(_, expr) => v.visit_expression(expr),
        pt::Expression::PreDecrement(_, expr) => v.visit_expression(expr),
        pt::Expression::UnaryPlus(_, expr) => v.visit_expression(expr),
        pt::Expression::UnaryMinus(_, expr) => v.visit_expression(expr),
        pt::Expression::Power(_, base, pow) => walk_binop(v, base, pow),
        pt::Expression::Multiply(_, left, right) => walk_binop(v, left, right),
        pt::Expression::Divide(_, left, right) => walk_binop(v, left, right),
        pt::Expression::Modulo(_, left, right) => walk_binop(v, left, right),
        pt::Expression::Add(_, left, right) => walk_binop(v, left, right),
        pt::Expression::Subtract(_, left, right) => walk_binop(v, left, right),
        pt::Expression::ShiftLeft(_, left, right) => walk_binop(v, left, right),
        pt::Expression::ShiftRight(_, left, right) => walk_binop(v, left, right),
        pt::Expression::BitwiseAnd(_, left, right) => walk_binop(v, left, right),
        pt::Expression::BitwiseXor(_, left, right) => walk_binop(v, left, right),
        pt::Expression::BitwiseOr(_, left, right) => walk_binop(v, left, right),
        pt::Expression::Less(_, left, right) => walk_binop(v, left, right),
        pt::Expression::More(_, left, right) => walk_binop(v, left, right),
        pt::Expression::LessEqual(_, left, right) => walk_binop(v, left, right),
        pt::Expression::MoreEqual(_, left, right) => walk_binop(v, left, right),
        pt::Expression::Equal(_, left, right) => walk_binop(v, left, right),
        pt::Expression::NotEqual(_, left, right) => walk_binop(v, left, right),
        pt::Expression::And(_, left, right) => walk_binop(v, left, right),
        pt::Expression::Or(_, left, right) => walk_binop(v, left, right),
        pt::Expression::Ternary(_, cond, tb, fb) => {
            v.visit_expression(cond)?;
            v.visit_expression(tb)?;
            v.visit_expression(fb)
        }
        pt::Expression::Assign(_, left, right) => walk_binop(v, left, right),
        pt::Expression::AssignOr(_, left, right) => walk_binop(v, left, right),
        pt::Expression::AssignAnd(_, left, right) => walk_binop(v, left, right),
        pt::Expression::AssignXor(_, left, right) => walk_binop(v, left, right),
        pt::Expression::AssignShiftLeft(_, left, right) => walk_binop(v, left, right),
        pt::Expression::AssignShiftRight(_, left, right) => walk_binop(v, left, right),
        pt::Expression::AssignAdd(_, left, right) => walk_binop(v, left, right),
        pt::Expression::AssignSubtract(_, left, right) => walk_binop(v, left, right),
        pt::Expression::AssignMultiply(_, left, right) => walk_binop(v, left, right),
        pt::Expression::AssignDivide(_, left, right) => walk_binop(v, left, right),
        pt::Expression::AssignModulo(_, left, right) => walk_binop(v, left, right),
        // BoolLiteral(Loc, bool)
        pt::Expression::BoolLiteral(..) => Ok(()),
        // NumberLiteral(Loc, String, String)
        pt::Expression::NumberLiteral(..) => Ok(()),
        // RationalNumberLiteral(Loc, String, String, String)
        pt::Expression::RationalNumberLiteral(..) => Ok(()),
        // HexNumberLiteral(Loc, String)
        pt::Expression::HexNumberLiteral(..) => Ok(()),
        // StringLiteral(Vec<StringLiteral>)
        pt::Expression::StringLiteral(..) => Ok(()),
        pt::Expression::Type(_, ty) => v.visit_type(ty),
        // HexLiteral(Vec<HexLiteral>) (I'm not sure what's going on here, either...)
        pt::Expression::HexLiteral(..) => Ok(()),
        // AddressLiteral(Loc, String)
        pt::Expression::AddressLiteral(..) => Ok(()),
        // Variable(Identifier)
        pt::Expression::Variable(..) => Ok(()),
        pt::Expression::List(_, params) => v.visit_parameterlist(params),
        pt::Expression::ArrayLiteral(_, exprs) => {
            exprs.iter().try_for_each(|expr| v.visit_expression(expr))
        }
        pt::Expression::Unit(_, expr, _) => v.visit_expression(expr),
        // This(Loc)
        pt::Expression::This(..) => Ok(()),
    }
}

// Helper, not a Solang type
fn walk_binop<V: Visitor + ?Sized>(
    v: &mut V,
    left: &pt::Expression,
    right: &pt::Expression,
) -> Result<()> {
    v.visit_expression(left)?;
    v.visit_expression(right)
}

// VariableDefinition(Loc, Expression, Vec<VariableAttribute>, Identifier, Option<Expression>)
pub fn walk_variable_definition<V: Visitor + ?Sized>(
    v: &mut V,
    var: &pt::VariableDefinition,
) -> Result<()> {
    v.visit_type_expression(&var.ty)?;
    var.initializer
        .iter()
        .try_for_each(|var| v.visit_expression(var))
}

// Vec<(Loc, Option<Parameter>)
pub fn walk_parameterlist<V: Visitor + ?Sized>(v: &mut V, pl: &pt::ParameterList) -> Result<()> {
    pl.iter()
        .try_for_each(|param| param.1.iter().try_for_each(|p| v.visit_parameter(p)))
}

// Parameter(Loc, Expression, Option<StorageLocation>, Option<Identifier>)
pub fn walk_parameter<V: Visitor + ?Sized>(v: &mut V, p: &pt::Parameter) -> Result<()> {
    v.visit_type_expression(&p.ty)
}

// TypeDefinition(Loc, Identifier, Expression)
pub fn walk_type_definition<V: Visitor + ?Sized>(
    v: &mut V,
    def: &pt::TypeDefinition,
) -> Result<()> {
    v.visit_type_expression(&def.ty)
}

// Using(Loc, UsingList, Option<Expression>, Option<Identifier>
pub fn walk_using<V: Visitor + ?Sized>(v: &mut V, u: &pt::Using) -> Result<()> {
    u.ty.iter().try_for_each(|ty| v.visit_type_expression(ty))
}

// Documented in match
pub fn walk_type<V: Visitor + ?Sized>(v: &mut V, ty: &pt::Type) -> Result<()> {
    match ty {
        pt::Type::Mapping(_, expr1, expr2) => {
            v.visit_expression(expr1)?;
            v.visit_expression(expr2)
        }
        // params: Vec<(Loc, Option<Parameter>) (why would this not be parameterlist type??)
        // attributes: Vec<FunctionAttribute>
        // returns: Option<ParameterList, Vec<FunctionAttribute>>
        pt::Type::Function {
            params, returns, ..
        } => {
            params
                .iter()
                .try_for_each(|param| param.1.iter().try_for_each(|p| v.visit_parameter(p)))?;
            returns
                .iter()
                .try_for_each(|rets| v.visit_parameterlist(&rets.0))
        }
        // Address, AddressPayable, Payable, Bool, String,
        // Int, Uint, Bytes, Rational, DynamicBytes
        _ => Ok(()),
    }
}
