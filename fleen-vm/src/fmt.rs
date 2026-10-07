//! Scalar formatting helpers (0.0.2 U13, F8).
//!
//! Provides a single source of truth for how int/float/bool values are
//! formatted to strings. Used by both the `ToStr` opcode (`e as string`)
//! and the `print` builtin, so the two paths never drift apart.

use crate::value::Value;

/// Format a scalar value (int, float, or bool) to its string representation.
///
/// - int: decimal, negative numbers include the `-` sign
/// - float: same format as Rust's `Display` for `f64`
/// - bool: `"true"` or `"false"`
///
/// Returns `None` if the value is not a supported scalar type
/// (string, unit, or func are not valid cast operands and are rejected
/// by typeck before reaching the VM).
pub fn fmt_scalar(v: &Value) -> Option<String> {
    match v {
        Value::Int(n) => Some(n.to_string()),
        Value::Float(f) => Some(f.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn int_positive() {
        assert_eq!(fmt_scalar(&Value::Int(42)), Some("42".to_string()));
    }

    #[test]
    fn int_negative() {
        assert_eq!(fmt_scalar(&Value::Int(-7)), Some("-7".to_string()));
    }

    #[test]
    fn int_zero() {
        assert_eq!(fmt_scalar(&Value::Int(0)), Some("0".to_string()));
    }

    #[test]
    fn float_whole() {
        // Anchored to exact text: must match what print renders (§"float as
        // string 与 print 的 float 输出一致"), Rust Display for f64 gives "3".
        assert_eq!(fmt_scalar(&Value::Float(3.0)), Some("3".to_string()));
    }

    #[test]
    fn float_fractional() {
        // Use a non-PI-approximation literal to keep clippy happy.
        assert_eq!(fmt_scalar(&Value::Float(1.5)), Some("1.5".to_string()));
    }

    #[test]
    fn float_negative() {
        assert_eq!(fmt_scalar(&Value::Float(-2.25)), Some("-2.25".to_string()));
    }

    #[test]
    fn bool_true() {
        assert_eq!(fmt_scalar(&Value::Bool(true)), Some("true".to_string()));
    }

    #[test]
    fn bool_false() {
        assert_eq!(fmt_scalar(&Value::Bool(false)), Some("false".to_string()));
    }

    #[test]
    fn non_scalar_returns_none() {
        assert_eq!(fmt_scalar(&Value::Str("hello".into())), None);
        assert_eq!(fmt_scalar(&Value::Unit), None);
        assert_eq!(
            fmt_scalar(&Value::Func(fleen_compiler::codegen::FuncId(0))),
            None
        );
    }
}
