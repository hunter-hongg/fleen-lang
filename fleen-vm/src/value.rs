//! Runtime value representation (BYTECODE.md §3.2).

use std::fmt;
use std::rc::Rc;

use fleen_compiler::codegen::FuncId;

/// A self-describing runtime value.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    /// 64-bit signed integer.
    Int(i64),
    /// 64-bit IEEE-754 float.
    Float(f64),
    /// Boolean.
    Bool(bool),
    /// Immutable shared string (0.0.1 temporary decision, BYTECODE.md §3.2).
    Str(Rc<str>),
    /// The unit value (no meaningful data).
    Unit,
    /// First-class function reference.
    Func(FuncId),
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Int(v) => write!(f, "{v}"),
            Value::Float(v) => write!(f, "{v}"),
            Value::Bool(v) => write!(f, "{v}"),
            Value::Str(s) => write!(f, "{s}"),
            Value::Unit => write!(f, "unit"),
            Value::Func(id) => write!(f, "<func #{}>", id.0),
        }
    }
}
