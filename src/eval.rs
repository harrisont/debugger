use crate::{
    command::grammar::EvalExpr,
    name_resolution::resolve_name_to_address,
    process::Process
};

pub struct EvalContext<'a> {
    pub process: &'a mut Process,
}

// TODO: Expression evaluation needs an evaluation context. Possibly includnig memory read, register read, and symbol names.
pub fn evaluate_expression(expr: EvalExpr, context: &mut EvalContext) -> Result<u64, String> {
    match expr {
        EvalExpr::Number(x) => Ok(x),
        EvalExpr::Add(x, _, y) => Ok(evaluate_expression(*x, context)? + evaluate_expression(*y, context)?),
        EvalExpr::Symbol(symbol) => {
            resolve_name_to_address(&symbol, context.process)
        }
    }
}