//! Typeck error types.

use crate::lexer::Span;
use crate::typeck::typed_hir::Type;
use std::fmt;

/// A single type error with span and kind.
#[derive(Debug, Clone, PartialEq)]
pub struct TypeckError {
    pub kind: TypeckErrorKind,
    pub span: Span,
}

impl TypeckError {
    pub fn new(kind: TypeckErrorKind, span: Span) -> Self {
        Self { kind, span }
    }
}

/// Kinds of type errors.
#[derive(Debug, Clone, PartialEq)]
pub enum TypeckErrorKind {
    /// Type mismatch between expected and found types.
    TypeMismatch { expected: Type, found: Type },
    /// Choose expression is not exhaustive.
    ChooseNotExhaustive {
        scrutinee_type: Type,
        missing_patterns: Vec<String>,
    },
    /// Invalid operand type for operator.
    InvalidOperand { op: String, ty: Type },
    /// Function call argument count mismatch.
    ArityMismatch { expected: usize, found: usize },
    /// Function call argument type mismatch.
    ArgTypeMismatch {
        index: usize,
        expected: Type,
        found: Type,
    },
    /// Condition must be Bool.
    ConditionNotBool { found: Type },
    /// Assignment type mismatch.
    AssignTypeMismatch { expected: Type, found: Type },
    /// Variable not found (should not happen after resolver).
    UndefinedVariable { name: String },
    /// Expression is not callable as a function.
    NotCallable { ty: Type },
    /// Feature parsed but not supported in 0.0.2.
    UnsupportedFeature { feature: String },
    /// Invariant violated (resolver guarantee broken); indicates a compiler bug.
    InternalError { message: String },
    /// Unsupported `as` cast: 0.0.2 only allows scalar → string.
    UnsupportedCast { from: Type, to: Type },
    /// `?` used in a function that does not return `Result<T, E>` (D6).
    QuestionOutsideResultFn,
    /// `?` applied to a non-Result operand.
    QuestionOnNonResult { found: Type },
    /// `?` operand's error type differs from the function's error type (D6:
    /// exact equality; no error-type conversion in 0.0.2).
    QuestionTypeMismatch { expected: Type, found: Type },
    /// `Ok(...)` / `Err(...)` with no context type to infer from (D6).
    CannotInferResultType { ctor: &'static str },
    /// A statement discards a `Result` value (Result may not be silently
    /// dropped — bind it, handle it with `choose`, or propagate with `?`).
    UnhandledResult { ty: Type },
    /// `Ok`/`Err` pattern used on a non-Result scrutinee.
    PatternTypeMismatch { pattern: &'static str, found: Type },
    // ---- 0.0.2 U05: ownership error family ----
    /// Use of a binding whose owned value was definitely moved earlier.
    UseAfterMove {
        name: String,
        use_span: Span,
        moved_span: Span,
    },
    /// Use of a binding that may be moved on some control-flow paths
    /// (branch join, loop back edge / exit).
    MaybeMovedAfterBranch {
        name: String,
        use_span: Span,
        moved_spans: Vec<Span>,
    },
    /// Assignment to a binding whose owned value was moved.
    /// Deliberately distinct from "undefined": the binding exists, but
    /// its slot was emptied by a move (DESIGN §10.2 rule 5/6).
    AssignToMoved { name: String },
    /// `move x` where `x` is a Copy type: moving is meaningless.
    MoveOfCopyType { name: String, ty: Type },
    /// `move deref b`: the box's unique ownership must not be broken by
    /// reading out its pointee.
    MoveOutOfBox,
    /// `move g` where `g` is an owned global: globals are read (cloned),
    /// never moved.
    MoveOutOfGlobal { name: String },
    /// `move s` where `s` is a `ref` parameter: borrowed values cannot be
    /// moved out.
    MoveOfBorrowed { name: String },
    /// The operand of `move` / `clone` is not a valid place: only a
    /// variable (or `deref <box>` for `clone`) may be moved/cloned.
    InvalidClonePlace,
    /// A bare owned value in a consuming position (binding/assignment RHS,
    /// argument, `choose` scrutinee, `box` inner, `Ok`/`Err` payload):
    /// ownership transfer must be explicit.
    OwnedArgRequiresMove { name: String, ty: Type },
    /// `move x` passed where a `ref T` parameter borrows: a borrow must not
    /// consume the value.
    BorrowArgWithMove { name: String },
    /// `deref b` passed where a `ref T` parameter borrows: 0.0.2 cannot
    /// borrow a box's pointee (DESIGN §10.4).
    BorrowOfBoxInterior,
    /// A temporary (not a variable) passed where a `ref T` parameter
    /// borrows: a borrow handle needs a slot to point at.
    RefArgNotAVariable,
    /// `deref g = v` where `g` is an owned global box: globals are read as
    /// deep copies, so the write could only reach a copy (0.0.2 limit;
    /// see DESIGN §10.3 / U12).
    DerefAssignOfGlobalBox,
}

impl fmt::Display for TypeckErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TypeckErrorKind::TypeMismatch { expected, found } => {
                write!(
                    f,
                    "type mismatch: expected `{}`, found `{}`",
                    expected.name(),
                    found.name()
                )
            }
            TypeckErrorKind::ChooseNotExhaustive {
                scrutinee_type,
                missing_patterns,
            } => {
                write!(
                    f,
                    "choose is not exhaustive for type `{}`: missing {}",
                    scrutinee_type.name(),
                    missing_patterns.join(", ")
                )
            }
            TypeckErrorKind::InvalidOperand { op, ty } => {
                write!(
                    f,
                    "invalid operand type `{}` for operator `{}`",
                    ty.name(),
                    op
                )
            }
            TypeckErrorKind::BorrowOfBoxInterior => {
                write!(
                    f,
                    "cannot borrow a box's pointee (`deref b` passed to a \
                     `ref` parameter) — 0.0.2 borrows only variables; help: \
                     copy the pointee out first (`t = clone deref b;`) or \
                     pass the box's owner"
                )
            }
            TypeckErrorKind::RefArgNotAVariable => {
                write!(
                    f,
                    "a `ref` parameter needs a variable to borrow (a local, \
                     a global, or another `ref` parameter) — a temporary \
                     value has no slot to point at; help: bind it to a \
                     variable first"
                )
            }
            TypeckErrorKind::ArityMismatch { expected, found } => {
                write!(f, "expected {} arguments, found {}", expected, found)
            }
            TypeckErrorKind::ArgTypeMismatch {
                index,
                expected,
                found,
            } => {
                write!(
                    f,
                    "argument {} type mismatch: expected `{}`, found `{}`",
                    index + 1,
                    expected.name(),
                    found.name()
                )
            }
            TypeckErrorKind::ConditionNotBool { found } => {
                write!(f, "condition must be `bool`, found `{}`", found.name())
            }
            TypeckErrorKind::AssignTypeMismatch { expected, found } => {
                write!(
                    f,
                    "cannot assign `{}` to variable of type `{}`",
                    found.name(),
                    expected.name()
                )
            }
            TypeckErrorKind::UndefinedVariable { name } => {
                write!(f, "undefined variable `{}`", name)
            }
            TypeckErrorKind::NotCallable { ty } => {
                write!(f, "cannot call value of type `{}`", ty.name())
            }
            TypeckErrorKind::UnsupportedFeature { feature } => {
                write!(f, "{} is not supported yet", feature)
            }
            TypeckErrorKind::InternalError { message } => {
                write!(f, "internal compiler error: {}", message)
            }
            TypeckErrorKind::UnsupportedCast { from, to } => {
                write!(
                    f,
                    "unsupported cast: cannot cast `{}` to `{}`; \
                     0.0.2 only supports scalar → string casts, \
                     full conversions arrive with generics (0.1.0)",
                    from.name(),
                    to.name()
                )
            }
            TypeckErrorKind::QuestionOutsideResultFn => {
                write!(
                    f,
                    "`?` can only be used in a function that returns `Result<T, E>`; \
                     help: change the function's return type to `Result<_, E>`, \
                     or handle the Result explicitly with `choose`"
                )
            }
            TypeckErrorKind::QuestionOnNonResult { found } => {
                write!(
                    f,
                    "`?` applies to a `Result<T, E>` value, found `{}`",
                    found.name()
                )
            }
            TypeckErrorKind::QuestionTypeMismatch { expected, found } => {
                write!(
                    f,
                    "`?` propagates error type `{}` but the function returns \
                     `Result<_, {}>`; help: error types must match exactly \
                     in 0.0.2 (no error-type conversion)",
                    found.name(),
                    expected.name()
                )
            }
            TypeckErrorKind::CannotInferResultType { ctor } => {
                write!(
                    f,
                    "cannot infer the type of `{ctor}(...)`: Result constructors \
                     take their type from context; help: add a type annotation, \
                     e.g. `res: Result<int, string> = {ctor}(…);`"
                )
            }
            TypeckErrorKind::UnhandledResult { ty } => {
                write!(
                    f,
                    "Result value `{}` is discarded; help: bind it, handle it \
                     with `choose`, or propagate it with `?`",
                    ty.name()
                )
            }
            TypeckErrorKind::PatternTypeMismatch { pattern, found } => {
                write!(
                    f,
                    "`{pattern}` pattern requires a `Result<T, E>` scrutinee, \
                     found `{}`",
                    found.name()
                )
            }
            TypeckErrorKind::UseAfterMove { name, .. } => {
                write!(
                    f,
                    "use of moved value `{name}`; help: clone the value if \
                     you still need it: `t = clone {name};`"
                )
            }
            TypeckErrorKind::MaybeMovedAfterBranch { name, .. } => {
                write!(
                    f,
                    "`{name}` may be moved in one of the branches; help: \
                     write `clone {name}` in the branch that moves it, or \
                     at the use site"
                )
            }
            TypeckErrorKind::AssignToMoved { name } => {
                write!(
                    f,
                    "cannot assign to moved value `{name}`; the binding \
                     still exists but its value was transferred — \
                     help: rebind with a fresh value or remove the earlier `move`"
                )
            }
            TypeckErrorKind::MoveOfCopyType { name, ty } => {
                write!(
                    f,
                    "`{name}` is `{}` (a Copy type): `move` is meaningless \
                     — help: drop the `move` keyword",
                    ty.name()
                )
            }
            TypeckErrorKind::MoveOutOfBox => {
                write!(
                    f,
                    "cannot move out of a box's pointee — help: \
                     use `clone deref b` to copy the pointee"
                )
            }
            TypeckErrorKind::MoveOutOfGlobal { name } => {
                write!(
                    f,
                    "cannot move out of global `{name}` — globals are \
                     read by value — help: use `clone {name}`"
                )
            }
            TypeckErrorKind::MoveOfBorrowed { name } => {
                write!(
                    f,
                    "cannot move out of borrowed value `{name}` (`ref` \
                     parameter) — help: the caller must pass an owned \
                     value with `move`"
                )
            }
            TypeckErrorKind::InvalidClonePlace => {
                write!(
                    f,
                    "`move` / `clone` operand must be a variable \
                     (`move x`) or a box pointee (`clone deref b`)"
                )
            }
            TypeckErrorKind::OwnedArgRequiresMove { name, ty } => {
                write!(
                    f,
                    "cannot use owned value `{name}` (`{}`) without \
                     transferring ownership — help: transfer it: `move {name}`, \
                     or copy it: `clone {name}`",
                    ty.name()
                )
            }
            TypeckErrorKind::BorrowArgWithMove { name } => {
                write!(
                    f,
                    "`ref` parameter borrows `{name}` but the argument \
                     transfers it — help: pass `{name}` without `move`"
                )
            }
            TypeckErrorKind::DerefAssignOfGlobalBox => {
                write!(
                    f,
                    "cannot assign through a global box: the global is read \
                     as a deep copy, so the write would only reach the copy \
                     (0.0.2 limit — see DESIGN §10.3)"
                )
            }
        }
    }
}

impl fmt::Display for TypeckError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "error: {}", self.kind)
    }
}
