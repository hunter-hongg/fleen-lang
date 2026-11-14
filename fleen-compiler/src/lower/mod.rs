//! Lower stage: TypedHir → MIR.
//!
//! Control flow (`if`/`elif`/`else`, `while`, `choose`, `and`/`or`) is
//! flattened into basic blocks connected by conditional jumps and jumps.
//! Locals live in numbered slots (no SSA); bindings are looked up via
//! `BindingId`. Shadowing is resolved at compile time into distinct slots.

pub mod cfg;
pub mod mir;
pub mod slots;

#[cfg(test)]
mod tests;

use self::cfg::FnBuilder;
use self::mir::*;
use self::slots::{SlotAlloc, allocate_slots};
use crate::lexer::Span;
use crate::parser::ast::ResultCtor;
use crate::resolver::hir::BindingId;
use crate::typeck::builtins::find_builtin;
use crate::typeck::typed_hir::Access;
use crate::typeck::typed_hir::*;
use std::collections::HashMap;

/// Lower a Typed HIR program into MIR.
///
/// # Arguments
/// - `typed_hir`: Type-checked HIR (output of `typeck`).
///
/// # Returns
/// The MIR module: functions (incl. builtin placeholders) + globals.
///
/// # Errors
/// Errors with `LowerError` on defensively-checked invariant violations
/// (constructs typeck already flags never reach lowering).
pub fn lower(typed_hir: TypedHir) -> Result<Mir, LowerError> {
    let mut lcx = LowerCtx::new(&typed_hir);
    lcx.lower_globals(&typed_hir.items)?;
    lcx.lower_funcs(&typed_hir.items)?;
    Ok(Mir {
        funcs: lcx.funcs,
        globals: lcx.globals,
        structs: typed_hir
            .structs
            .into_iter()
            .map(|def| MirStructDef {
                field_types: def.field_types,
            })
            .collect(),
        enums: typed_hir
            .enums
            .into_iter()
            .map(|def| MirEnumDef {
                variants: def
                    .variants
                    .into_iter()
                    .map(|v| MirEnumVariantDef {
                        payload_types: v.payload_types,
                    })
                    .collect(),
                // 0.0.7 T07/D8: the arity rides through MIR so codegen can
                // serialize it — it is not derivable from the payloads.
                type_param_count: def.type_params.len() as u32,
            })
            .collect(),
    })
}

/// An error during lowering.
#[derive(Debug, Clone, PartialEq)]
pub struct LowerError {
    pub kind: LowerErrorKind,
    pub span: crate::lexer::Span,
}

/// Kinds of lowering errors.
#[derive(Debug, Clone, PartialEq)]
pub enum LowerErrorKind {
    /// A feature that the parser/typeck accept but the current lower stage
    /// cannot lower yet (e.g. pending U06 bytecode support).
    UnsupportedFeature { feature: &'static str },
    /// A global initializer with control flow (if/while/choose/block).
    ComplexGlobalInit,
    /// An indirect (function-value) call in a global initializer.
    IndirectCallInGlobalInit,
    /// An identifier resolved to no known slot/global/function.
    UndeclaredBinding { name: String },
    /// Negation or arithmetic on an incompatible type (typeck invariant broken).
    InvalidOperand { op: &'static str },
    /// 0.0.2 U06: a `ref` argument passed to a global initializer call.
    /// Global init has no frame to host a temp slot for global ref args.
    RefArgInGlobalInit { name: String },
    /// 0.0.2 U06: a `choose` on a Result scrutinee must have both
    /// `when Ok(..)` and `when Err(..)` arms (no catchall/wildcard).
    ChooseResultNeedsOkErrArms,
    /// 0.0.2 U06: defensive: typeck rejects deref-assign of global boxes.
    /// This ensures lower stays total for malformed trees.
    GlobalBoxDerefAssign { name: String },
}

impl LowerError {
    fn new(kind: LowerErrorKind, span: crate::lexer::Span) -> Self {
        LowerError { kind, span }
    }
}

impl std::fmt::Display for LowerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.kind {
            LowerErrorKind::UnsupportedFeature { feature } => {
                write!(f, "unsupported feature: {feature}")
            }
            LowerErrorKind::ComplexGlobalInit => {
                write!(f, "global initializer must be a simple expression")
            }
            LowerErrorKind::IndirectCallInGlobalInit => {
                write!(f, "indirect calls are not supported in global initializers")
            }
            LowerErrorKind::UndeclaredBinding { name } => {
                write!(f, "undeclared binding: {name}")
            }
            LowerErrorKind::InvalidOperand { op } => {
                write!(f, "invalid operand for operator {op}")
            }
            LowerErrorKind::RefArgInGlobalInit { name } => {
                write!(
                    f,
                    "global initializer cannot take a `ref` argument ({name})"
                )
            }
            LowerErrorKind::ChooseResultNeedsOkErrArms => {
                write!(
                    f,
                    "a `choose` on a Result requires both `when Ok(..)` and `when Err(..)` arms"
                )
            }
            LowerErrorKind::GlobalBoxDerefAssign { name } => {
                write!(f, "cannot deref-assign a global box ({name})")
            }
        }
    }
}

impl std::error::Error for LowerError {}

// Builtin declarations live in `crate::typeck::builtins::BUILTINS` (single
// source of truth for names, arities and signatures); lower mirrors them
// into leading `MirFunc`s in `LowerCtx::new`.

/// Where a binding lives at runtime.
enum Location {
    Local(u16),
    Global(GlobalId),
}

struct LowerCtx {
    funcs: Vec<MirFunc>,
    globals: Vec<MirGlobal>,
    /// Function name → FuncId (function names are scope-unique in 0.0.1).
    func_ids: HashMap<String, FuncId>,
    /// BindingId → location for global variables.
    global_of_binding: HashMap<BindingId, GlobalId>,
    /// FuncId → parameter types (for `ref` argument detection in calls).
    /// Builtins have an empty vec (no Ref parameters).
    param_types: HashMap<FuncId, Vec<Type>>,
    /// Variadic builtins get one `MirFunc` per observed call arity: the
    /// `Call` instruction pops exactly `callee.params` operands, so the
    /// arity must be encoded in the callee's `params` field. Maps
    /// (name, argc) → FuncId; the base instance registered in `new` covers
    /// its own arity.
    builtin_arities: HashMap<(String, u16), FuncId>,
}

impl LowerCtx {
    /// FuncId of a variadic builtin instance that takes exactly `argc`
    /// arguments, instantiating it on first use (U09 fix: the `Call`
    /// instruction pops `callee.params` operands, so a `print("a", b)` call
    /// must target an instance whose `params == 2` — the shared
    /// `params == 1` base instance left the extra argument on the stack and
    /// desynced every depth the verifier reasons about).
    fn variadic_builtin_fid(&mut self, name: &str, argc: u16) -> FuncId {
        if let Some(&fid) = self.builtin_arities.get(&(name.to_string(), argc)) {
            return fid;
        }
        let ret = find_builtin(name).map(|sig| sig.ret).unwrap_or(Type::Unit);
        let id = FuncId(self.funcs.len() as u32);
        self.funcs.push(MirFunc {
            func_id: id,
            name: name.to_string(),
            params: argc,
            locals: argc.max(1),
            entry: BlockId(0),
            blocks: vec![MirBlock {
                id: BlockId(0),
                instrs: vec![
                    // Builtin stub bodies have no source location; the encoder skips
                    // synthetic spans when building the span_map.
                    MirInstr::new(MirInstrKind::Unit, Span::SYNTHETIC),
                ],
                terminator: Terminator::Return,
            }],
            is_builtin: true,
            ret_type: ret,
        });
        self.builtin_arities.insert((name.to_string(), argc), id);
        id
    }

    fn new(hir: &TypedHir) -> Self {
        let mut ctx = LowerCtx {
            funcs: Vec::new(),
            globals: Vec::new(),
            func_ids: HashMap::new(),
            global_of_binding: HashMap::new(),
            param_types: HashMap::new(),
            builtin_arities: HashMap::new(),
        };
        // Builtin functions occupy leading FuncIds.
        for sig in crate::typeck::builtins::builtins() {
            let id = FuncId(ctx.funcs.len() as u32);
            // The base instance's `params` must equal the signature's
            // parameter count: the `Call` instruction pops `callee.params`
            // operands (U09). Variadic builtins grow extra instances lazily
            // per observed arity (see `variadic_builtin_fid`).
            let params: u16 = match &sig.param {
                crate::typeck::builtins::BuiltinParam::Fixed(p) => p.len() as u16,
                crate::typeck::builtins::BuiltinParam::Variadic(_) => 1,
                crate::typeck::builtins::BuiltinParam::Schematic { params, .. } => {
                    params.len() as u16
                }
            };
            ctx.func_ids.insert(sig.name.to_string(), id);
            ctx.param_types.insert(id, Vec::new()); // builtins have no Ref params
            ctx.funcs.push(MirFunc {
                func_id: id,
                name: sig.name.to_string(),
                params,
                // The verifier requires `locals >= params`: each parameter
                // occupies a local slot (U09 stack shape).
                locals: params.max(1),
                entry: BlockId(0),
                blocks: vec![MirBlock {
                    id: BlockId(0),
                    instrs: vec![
                        // Builtin stub bodies have no source location; the encoder skips
                        // synthetic spans when building the span_map.
                        MirInstr::new(MirInstrKind::Unit, Span::SYNTHETIC),
                    ],
                    terminator: Terminator::Return,
                }],
                is_builtin: true,
                ret_type: sig.ret.clone(),
            });
            // The base instance serves call sites of its own arity; other
            // arities (variadic only, in practice) instantiate lazily.
            ctx.builtin_arities
                .insert((sig.name.to_string(), params), id);
        }
        // Assign FuncIds to user functions in declaration order.
        for item in &hir.items {
            if let TypedHirItem::Decl(TypedDeclHir::Func(f)) = item {
                let id = FuncId(ctx.funcs.len() as u32);
                ctx.func_ids.insert(f.name.clone(), id);
                let param_tys: Vec<Type> = f.params.iter().map(|p| p.ty.clone()).collect();
                ctx.param_types.insert(id, param_tys);
                ctx.funcs.push(MirFunc {
                    func_id: id,
                    name: f.name.clone(),
                    params: f.params.len() as u16,
                    locals: 0, // filled after slot allocation
                    entry: BlockId(0),
                    blocks: Vec::new(),
                    is_builtin: false,
                    ret_type: f.ret_type.clone().unwrap_or(Type::Unit),
                });
            }
        }
        ctx
    }

    fn lower_globals(&mut self, items: &[TypedHirItem]) -> Result<(), LowerError> {
        for item in items {
            let (name, mutable, binding_id, init_expr) = match item {
                TypedHirItem::Decl(TypedDeclHir::Var(v)) => {
                    (v.name.clone(), true, v.binding_id, &v.init)
                }
                TypedHirItem::Decl(TypedDeclHir::Const(c)) => {
                    (c.name.clone(), false, c.binding_id, &c.init)
                }
                _ => continue,
            };
            let gid = GlobalId(self.globals.len() as u32);
            self.global_of_binding.insert(binding_id, gid);
            let mut init = Vec::new();
            lower_global_init(self, init_expr, &mut init)?;
            self.globals.push(MirGlobal {
                global_id: gid,
                name,
                mutable,
                ty: init_expr.ty(),
                init,
            });
        }
        Ok(())
    }

    fn lower_funcs(&mut self, items: &[TypedHirItem]) -> Result<(), LowerError> {
        for item in items {
            let TypedHirItem::Decl(TypedDeclHir::Func(f)) = item else {
                continue;
            };
            let fid = self.func_ids[&f.name].0 as usize;
            let slots = allocate_slots(f);
            let mut fb = FnBuilder::new();
            let mut bcx = FnBodyCx {
                lcx: &mut *self,
                slots,
                is_async: f.is_async,
            };
            // 0.0.4 T05: an `async func`'s code begins with `AsyncStart`, the
            // call boundary that packages the frame's parameters into a
            // not-yet-started coroutine frame and returns the `Future` to the
            // caller. It must be the first instruction of the entry block so
            // the bytecode offset of the body entry is fixed (pc + 1); the
            // body that follows is driven by `Await` via `resume_frame`.
            if f.is_async {
                fb.emit(MirInstr::new(MirInstrKind::AsyncStart, f.span));
            }
            match &f.body {
                TypedFuncBodyHir::SingleExpr(e) => bcx.lower_expr_into(&mut fb, e)?,
                TypedFuncBodyHir::Block(b) => bcx.lower_block(&mut fb, b)?,
            }
            fb.end(Terminator::Return);
            let locals = bcx.slots.total();
            self.funcs[fid].locals = locals;
            let (entry, blocks) = fb.finish();
            self.funcs[fid].entry = entry;
            self.funcs[fid].blocks = blocks;
        }
        Ok(())
    }
}

/// Lower a global initializer to a flat instruction list.
///
/// Control-flow-bearing initializers are rejected: 0.0.1 requires global
/// initializers to be "simple" (literals, arithmetic, calls, identifiers).
fn lower_global_init(
    lcx: &LowerCtx,
    expr: &TypedExprHir,
    out: &mut Vec<MirInstr>,
) -> Result<(), LowerError> {
    match expr {
        TypedExprHir::Int(v, _) => out.push(MirInstr::new(MirInstrKind::ConstInt(*v), expr.span())),
        TypedExprHir::Float(v, _) => {
            out.push(MirInstr::new(MirInstrKind::ConstFloat(*v), expr.span()))
        }
        TypedExprHir::Str(v, _) => out.push(MirInstr::new(
            MirInstrKind::ConstStr(v.clone()),
            expr.span(),
        )),
        TypedExprHir::Bool(true, _) => out.push(MirInstr::new(MirInstrKind::True, expr.span())),
        TypedExprHir::Bool(false, _) => out.push(MirInstr::new(MirInstrKind::False, expr.span())),
        TypedExprHir::Ident {
            name, binding_id, ..
        } => {
            if let Some(&gid) = lcx.global_of_binding.get(binding_id) {
                out.push(MirInstr::new(
                    MirInstrKind::LoadGlobal(gid.0 as u16),
                    expr.span(),
                ));
            } else if let Some(&fid) = lcx.func_ids.get(name) {
                out.push(MirInstr::new(MirInstrKind::LoadFunc(fid), expr.span()));
            } else {
                return Err(LowerError::new(
                    LowerErrorKind::UndeclaredBinding { name: name.clone() },
                    expr.span(),
                ));
            }
        }
        TypedExprHir::Neg(e) => {
            lower_global_init(lcx, e, out)?;
            match e.ty() {
                Type::Int => out.push(MirInstr::new(MirInstrKind::NegI, expr.span())),
                Type::Float => out.push(MirInstr::new(MirInstrKind::NegF, expr.span())),
                _ => {
                    return Err(LowerError::new(
                        LowerErrorKind::InvalidOperand { op: "neg" },
                        e.span(),
                    ));
                }
            }
        }
        TypedExprHir::Not(e) => {
            lower_global_init(lcx, e, out)?;
            out.push(MirInstr::new(MirInstrKind::Not, expr.span()));
        }
        TypedExprHir::Add(l, r)
        | TypedExprHir::Sub(l, r)
        | TypedExprHir::Mul(l, r)
        | TypedExprHir::Div(l, r)
        | TypedExprHir::Mod(l, r) => {
            lower_global_init(lcx, l, out)?;
            lower_global_init(lcx, r, out)?;
            let is_int = matches!(l.ty(), Type::Int);
            let op = match expr {
                TypedExprHir::Add(..) => {
                    if is_int {
                        MirInstrKind::IAdd
                    } else {
                        MirInstrKind::FAdd
                    }
                }
                TypedExprHir::Sub(..) => {
                    if is_int {
                        MirInstrKind::ISub
                    } else {
                        MirInstrKind::FSub
                    }
                }
                TypedExprHir::Mul(..) => {
                    if is_int {
                        MirInstrKind::IMul
                    } else {
                        MirInstrKind::FMul
                    }
                }
                TypedExprHir::Div(..) => {
                    if is_int {
                        MirInstrKind::IDiv
                    } else {
                        MirInstrKind::FDiv
                    }
                }
                _ => {
                    if matches!(l.ty(), Type::Int) && matches!(r.ty(), Type::Int) {
                        MirInstrKind::IMod
                    } else {
                        return Err(LowerError::new(
                            LowerErrorKind::InvalidOperand { op: "mod" },
                            expr.span(),
                        ));
                    }
                }
            };
            out.push(MirInstr::new(op, expr.span()));
        }
        TypedExprHir::Eq(l, r)
        | TypedExprHir::Ne(l, r)
        | TypedExprHir::Lt(l, r)
        | TypedExprHir::Gt(l, r)
        | TypedExprHir::Le(l, r)
        | TypedExprHir::Ge(l, r) => {
            lower_global_init(lcx, l, out)?;
            lower_global_init(lcx, r, out)?;
            out.push(MirInstr::new(
                match expr {
                    TypedExprHir::Eq(..) => MirInstrKind::Eq,
                    TypedExprHir::Ne(..) => MirInstrKind::Ne,
                    TypedExprHir::Lt(..) => MirInstrKind::Lt,
                    TypedExprHir::Gt(..) => MirInstrKind::Gt,
                    TypedExprHir::Le(..) => MirInstrKind::Le,
                    _ => MirInstrKind::Ge,
                },
                expr.span(),
            ));
        }
        TypedExprHir::Call(callee, args, _) => match callee.as_ref() {
            TypedExprHir::Ident { name, .. } if lcx.func_ids.contains_key(name) => {
                // 0.0.2 U06: check for Ref arguments in global init calls.
                let fid = lcx.func_ids[name];
                if let Some(param_tys) = lcx.param_types.get(&fid) {
                    for (i, arg) in args.iter().enumerate() {
                        if param_tys.get(i).is_some_and(|t| matches!(t, Type::Ref(_))) {
                            return Err(LowerError::new(
                                LowerErrorKind::RefArgInGlobalInit { name: name.clone() },
                                arg.span(),
                            ));
                        }
                    }
                }
                for a in args {
                    lower_global_init(lcx, a, out)?;
                }
                out.push(MirInstr::new(MirInstrKind::Call(fid), expr.span()));
            }
            _ => {
                return Err(LowerError::new(
                    LowerErrorKind::IndirectCallInGlobalInit,
                    expr.span(),
                ));
            }
        },
        // 0.0.2 U13: scalar → string in a global initializer.
        TypedExprHir::Cast(inner, _) => {
            lower_global_init(lcx, inner, out)?;
            out.push(MirInstr::new(MirInstrKind::ToStr, expr.span()));
        }
        // 0.0.2 U06: `box e` in a global initializer.
        TypedExprHir::Box(inner, span) => {
            lower_global_init(lcx, inner, out)?;
            out.push(MirInstr::new(MirInstrKind::AllocBox, *span));
        }
        // 0.0.3 V02: struct literal in a global initializer. Global init
        // has no frame, so there are no temp slots for the reorder — only
        // declaration-ordered initializers can be lowered here; anything
        // else stays a `ComplexGlobalInit`.
        TypedExprHir::StructLit {
            struct_id,
            inits,
            span,
            ..
        } => {
            let in_decl_order = inits_in_decl_order(inits);
            if !in_decl_order {
                return Err(LowerError::new(LowerErrorKind::ComplexGlobalInit, *span));
            }
            for (_, init) in inits {
                lower_global_init(lcx, init, out)?;
            }
            out.push(MirInstr::new(
                MirInstrKind::StructNew(struct_id.0 as u16),
                *span,
            ));
        }
        // 0.0.3 V03: array literal in a global initializer. Elements
        // evaluate in source order (always the declaration order for
        // arrays), so no reorder is needed — unlike struct literals, any
        // element shapes that `lower_global_init` accepts compose here.
        TypedExprHir::ArrayLit { elems, span, .. } => {
            let n = arr_len_u16(elems.len(), *span)?;
            for e in elems {
                lower_global_init(lcx, e, out)?;
            }
            out.push(MirInstr::new(MirInstrKind::ArrNew(n), *span));
        }
        _ => {
            return Err(LowerError::new(
                LowerErrorKind::ComplexGlobalInit,
                expr.span(),
            ));
        }
    }
    Ok(())
}

/// Whether a struct literal's initializers are already in declaration
/// order: typeck pairs each initializer with its declaration index, so
/// declaration order means the indices are `0..n` in sequence.
fn inits_in_decl_order(inits: &[(u16, TypedExprHir)]) -> bool {
    inits
        .iter()
        .enumerate()
        .all(|(i, (idx, _))| *idx == i as u16)
}

/// Fit an array literal's element count into the `ArrNew` u16 operand.
fn arr_len_u16(n: usize, span: Span) -> Result<u16, LowerError> {
    u16::try_from(n).map_err(|_| {
        LowerError::new(
            LowerErrorKind::UnsupportedFeature {
                feature: "array literal with more than 65535 elements",
            },
            span,
        )
    })
}

/// Source span of a `choose` arm: the pattern's span when it has one,
/// otherwise the arm body's (only `TypedPatternHir::Error` carries none,
/// and that only appears in error-recovery trees).
fn choose_arm_span(arm: &TypedChooseArmHir) -> Span {
    match &arm.pattern {
        TypedPatternHir::Error => arm.body.span,
        p => p.span(),
    }
}

/// Return the `Dup` or `DupDeep` instruction kind for a value of type `ty`.
fn dup_kind(ty: &Type) -> MirInstrKind {
    if ty.is_owned() {
        MirInstrKind::DupDeep
    } else {
        MirInstrKind::Dup
    }
}

/// Concrete payload type of enum `variant_index`, payload `payload_index`,
/// for a scrutinee of type `scrutinee_ty` (substituting type parameters for
/// `Type::App`).
///
/// # Panics
/// Panics when `scrutinee_ty` is not the enum type the pattern was checked
/// against, or the indices are out of range — typeck guarantees a
/// well-formed, arity-checked pattern tree, so either is a compiler bug and
/// must not be papered over with a wrong-type lowering.
fn enum_payload_ty(scrutinee_ty: &Type, variant_index: u32, payload_index: usize) -> Type {
    match scrutinee_ty {
        Type::Enum(info) => info
            .payloads
            .get(variant_index as usize)
            .and_then(|v| v.get(payload_index))
            .cloned()
            .unwrap_or_else(|| unreachable!("pattern payload indices are arity-checked by typeck")),
        Type::App(info, args) => {
            let skel = info
                .payloads
                .get(variant_index as usize)
                .and_then(|v| v.get(payload_index))
                .unwrap_or_else(|| {
                    unreachable!("pattern payload indices are arity-checked by typeck")
                });
            crate::typeck::typed_hir::substitute_params(skel, args)
        }
        _ => unreachable!("enum pattern lowering only runs on a typeck-approved enum scrutinee"),
    }
}

/// Concrete Ok/Err payload type of a `Result<T, E>` scrutinee.
///
/// # Panics
/// Panics when `scrutinee_ty` is not a `Result` — the choose dispatch sends
/// only `Result` scrutinees to the `Ok`/`Err` lowering paths, so anything
/// else is a compiler bug.
fn result_payload_ty(scrutinee_ty: &Type, ctor: ResultCtor) -> Type {
    match scrutinee_ty {
        Type::Result(t, e) => match ctor {
            ResultCtor::Ok => (**t).clone(),
            ResultCtor::Err => (**e).clone(),
        },
        _ => {
            unreachable!("Ok/Err pattern lowering only runs on a typeck-approved Result scrutinee")
        }
    }
}

/// Strip type guards (D7: compile-time only) to reach the underlying pattern.
fn strip_type_guard(pat: &TypedPatternHir) -> &TypedPatternHir {
    let mut cur = pat;
    while let TypedPatternHir::TypeGuard { pattern, .. } = cur {
        cur = pattern;
    }
    cur
}

/// A flattened choose alternative (0.0.7 T06): one match path, possibly
/// expanded from an or-pattern arm. `arm_index` is the alternative's owning
/// arm's position in `arms` (body blocks are shared per arm).
struct Alt<'a> {
    pat: &'a TypedPatternHir,
    guard: Option<&'a TypedExprHir>,
    arm_index: usize,
}

/// Scrutinee context for the enum `choose` lowering helpers: the parked
/// value's type, its source span, and the copy instruction its type needs.
struct EnumScr<'a> {
    ty: &'a Type,
    span: Span,
    dup: MirInstrKind,
}

/// Failure edge threaded through recursive sub-pattern matching (0.0.7 T06):
/// where control goes on a mismatch and how many re-parked values sit above
/// the caller's base and must be cleaned before jumping there.
struct FailEdge {
    on_fail: BlockId,
    extra_below: u32,
}

/// Emit the failure-cleanup block a mismatched subject copy jumps to: pop
/// the subject copy, pop the caller's `extra_below` re-parked values, then
/// jump to `fail.on_fail`. Returns the block id (the caller resolves its
/// conditional jump to it, or fallthrough lands on it directly).
fn fail_cleanup_block(fb: &mut FnBuilder, span: Span, fail: &FailEdge) -> BlockId {
    let fblk = fb.new_block();
    fb.start(fblk);
    fb.emit(MirInstr::new(MirInstrKind::Pop, span));
    pop_parked(fb, fail.extra_below, span);
    fb.jump(fail.on_fail);
    fblk
}

/// Pop `n` re-parked values (failure cleanup — see [`FailEdge`]).
fn pop_parked(fb: &mut FnBuilder, n: u32, span: Span) {
    for _ in 0..n {
        fb.emit(MirInstr::new(MirInstrKind::Pop, span));
    }
}

/// Shared shape of a leaf sub-pattern test (literal equality, enum variant
/// discriminant): `test` emits the subject copy plus the comparison and
/// returns its pending conditional jump; the helper then lays out the
/// matched continuation and the failure-cleanup block so the conditional's
/// untaken edge falls through into the matched block and the taken edge
/// lands on cleanup. On return, the current block is the continuation.
fn emit_leaf_test(
    fb: &mut FnBuilder,
    span: Span,
    fail: &FailEdge,
    test: impl FnOnce(&mut FnBuilder) -> Result<BlockId, LowerError>,
) -> Result<(), LowerError> {
    fb.spill_for_fallthrough();
    let fail_jmp = test(fb)?;
    // match falls through: the jump pops the bool, the Pop consumes the subject
    let matched = fb.new_block();
    fb.start(matched);
    fb.emit(MirInstr::new(MirInstrKind::Pop, span));
    let cont = fb.new_block();
    fb.jump(cont);
    let fblk = fail_cleanup_block(fb, span, fail);
    fb.resolve(fail_jmp, fblk);
    fb.start(cont);
    Ok(())
}

/// Start the block every pending failure jump of the generic choose chain
/// resolves to (the next alternative's test point). A no-op when `pending`
/// is empty: the previous block already ended in an explicit jump, so the
/// current block stays the continuation point.
fn start_pending_block(fb: &mut FnBuilder, pending: &mut Vec<BlockId>) {
    if pending.is_empty() {
        return;
    }
    let next = fb.new_block();
    fb.start(next);
    for cond_block in pending.drain(..) {
        fb.resolve(cond_block, next);
    }
}

/// Shape of a `for` iterable, decided once before the desugar.
///
/// The closed set `{ [T], Range }` is a typeck invariant (DESIGN.md D4);
/// any other type reaching lowering is a defensive `UnsupportedFeature`.
enum IterKind {
    /// `[T]`: index-driven iteration, element `i` read as a copy.
    Array,
    /// `Range`: plain integer loop from start to end.
    Range,
}

impl IterKind {
    /// Classify the iterable expression's type.
    fn of(iterable: &TypedExprHir) -> Option<IterKind> {
        match iterable.ty() {
            Type::Array(_) => Some(IterKind::Array),
            Type::Range => Some(IterKind::Range),
            _ => None,
        }
    }
}

/// Which `std.collection.iter` helper an inline call lowers to (0.0.3 V07).
///
/// Each maps to a compiler-generated loop; the schema (which argument is
/// the function, whether an array accumulator is produced) differs per
/// helper, so the lowering is not one template but four small ones sharing
/// the loop skeleton.
///
/// `zip` is deliberately absent: 0.0.3 has no tuples, so an eager pair has
/// no representation (the V07 ticket's review note, PLAN D8/§6.5).
enum InlineIterKind {
    /// `map(xs, f)`: `[U]` built by pushing `f(elem)` for each elem.
    Map,
    /// `filter(xs, pred)`: `[T]` built by pushing elems where `pred` is true.
    Filter,
    /// `foldl(xs, init, f)`: a single accumulator (`f(acc, elem)` each
    /// step). 0.0.6 T07: renamed from `fold` (D5/D6 — direction-explicit,
    /// no alias).
    Fold,
    /// `foldr(xs, init, f)`: a single accumulator walking the array
    /// **backward** by index (`f(elem, acc)` each step — the T/A argument
    /// order is reversed relative to `foldl`, D5). No reverse copy: the
    /// loop index counts down.
    FoldR,
    /// `take(xs, n)`: the first `min(n, len(xs))` elems, unchanged.
    Take,
}

impl InlineIterKind {
    /// Classify an inline-lowered helper by its qualified builtin name.
    fn of(name: &str) -> Option<InlineIterKind> {
        match name {
            "std.collection.iter.map" => Some(Self::Map),
            "std.collection.iter.filter" => Some(Self::Filter),
            // 0.0.6 T07 (D5/D6): `fold` is now `foldl`, and `foldr` joins.
            "std.collection.iter.foldl" => Some(Self::Fold),
            "std.collection.iter.foldr" => Some(Self::FoldR),
            "std.collection.iter.take" => Some(Self::Take),
            _ => None,
        }
    }

    /// Is the function-valued argument the last (index `args.len() - 1`)?
    ///
    /// `map`/`filter` take it second, `foldl`/`foldr` take it third
    /// (`init` second); `take` has no function argument.
    fn fn_arg_index(&self) -> Option<usize> {
        match self {
            Self::Map | Self::Filter => Some(1),
            Self::Fold | Self::FoldR => Some(2),
            Self::Take => None,
        }
    }

    /// 0.0.6 T07: is this a fold (single accumulator, no output array)?
    fn is_fold(&self) -> bool {
        matches!(self, Self::Fold | Self::FoldR)
    }

    /// 0.0.6 T07 review: the backward-fold predicate, sibling of `is_fold`.
    fn is_foldr(&self) -> bool {
        matches!(self, Self::FoldR)
    }
}

/// Per-function lowering context.
struct FnBodyCx<'a> {
    lcx: &'a mut LowerCtx,
    slots: SlotAlloc,
    /// Whether the enclosing function is `async` (0.0.4 T06): suspend-source
    /// builtins (`std.io.sleep` / `std.async.yield_now`) lower to a trailing
    /// `Suspend` only inside an async body; in a sync body they are no-ops.
    is_async: bool,
}

impl<'a> FnBodyCx<'a> {
    /// Resolve where a binding lives. Function bindings are never dereferenced
    /// directly — they are treated specially (LoadFunc / direct Call).
    fn loc(
        &self,
        binding_id: BindingId,
        name: &str,
        span: crate::lexer::Span,
    ) -> Result<Location, LowerError> {
        if let Some(slot) = self.slots.get(binding_id) {
            Ok(Location::Local(slot))
        } else if let Some(&gid) = self.lcx.global_of_binding.get(&binding_id) {
            Ok(Location::Global(gid))
        } else {
            // 0.0.3 V05: imported globals arrive under their mangled name and
            // have a global slot but no local BindingId from the merged
            // type pass, so name-match against the globals table too.
            if let Some((idx, _)) = self
                .lcx
                .globals
                .iter()
                .enumerate()
                .find(|(_, g)| g.name == name)
            {
                return Ok(Location::Global(GlobalId(idx as u32)));
            }
            Err(LowerError::new(
                LowerErrorKind::UndeclaredBinding {
                    name: name.to_string(),
                },
                span,
            ))
        }
    }

    /// A binding is a *function* binding iff it owns neither a local slot
    /// nor a global entry (params/vars/consts/patterns have slots; only
    /// top-level functions live elsewhere).
    fn is_function_binding(&self, binding_id: BindingId, name: &str) -> bool {
        if self.slots.get(binding_id).is_some() {
            return false;
        }
        if self.lcx.global_of_binding.contains_key(&binding_id) {
            return false;
        }
        !self.lcx.globals.iter().any(|g| g.name == name)
    }

    /// Allocate a fresh temporary slot for global ref arguments.
    fn fresh_temp(&mut self) -> u16 {
        self.slots.fresh()
    }

    /// Lower a block (statements then tail expression).
    ///
    /// On exit the block's value (tail expr, or `Unit`) is on the stack.
    fn lower_block(&mut self, fb: &mut FnBuilder, block: &TypedBlockHir) -> Result<(), LowerError> {
        let n = block.stmts.len();
        for (i, stmt) in block.stmts.iter().enumerate() {
            match stmt {
                TypedStmtHir::Decl(TypedDeclHir::Var(v)) => {
                    self.slots.slot_for(v.binding_id);
                    self.lower_expr_into(fb, &v.init)?;
                    // Invariant: slot_for above just allocated it.
                    let slot = self.slots.get(v.binding_id).expect("slot just allocated");
                    fb.emit(MirInstr::new(MirInstrKind::StoreLocal(slot), v.span));
                }
                TypedStmtHir::Decl(TypedDeclHir::Const(c)) => {
                    self.slots.slot_for(c.binding_id);
                    self.lower_expr_into(fb, &c.init)?;
                    // Invariant: slot_for above just allocated it.
                    let slot = self.slots.get(c.binding_id).expect("slot just allocated");
                    fb.emit(MirInstr::new(MirInstrKind::StoreLocal(slot), c.span));
                }
                TypedStmtHir::Decl(TypedDeclHir::Func(_)) => {
                    // Functions are hoisted; nothing emitted inline.
                }
                TypedStmtHir::Expr(e, has_semi) => {
                    self.lower_expr_into(fb, e)?;
                    let is_last_and_no_tail = i + 1 == n && block.tail_expr.is_none();
                    if *has_semi || !is_last_and_no_tail {
                        fb.emit(MirInstr::new(MirInstrKind::Pop, e.span()));
                    }
                }
                TypedStmtHir::Error => {}
            }
        }
        match &block.tail_expr {
            Some(tail) => self.lower_expr_into(fb, tail)?,
            None => fb.emit(MirInstr::new(MirInstrKind::Unit, block.span)),
        }
        Ok(())
    }

    /// Lower an expression; its value is left on the stack.
    fn lower_expr_into(
        &mut self,
        fb: &mut FnBuilder,
        expr: &TypedExprHir,
    ) -> Result<(), LowerError> {
        match expr {
            // 0.0.6 T06: lower the payload arguments left-to-right, then
            // emit EnumNew with the enum/variant ids from the typed HIR.
            TypedExprHir::EnumCtor {
                enum_id,
                variant_idx,
                args,
                span,
                ..
            } => {
                for a in args {
                    self.lower_expr_into(fb, a)?;
                }
                fb.emit(MirInstr::new(
                    MirInstrKind::EnumNew(enum_id.0, *variant_idx),
                    *span,
                ));
            }
            TypedExprHir::Int(v, _) => {
                fb.emit(MirInstr::new(MirInstrKind::ConstInt(*v), expr.span()))
            }
            TypedExprHir::Float(v, _) => {
                fb.emit(MirInstr::new(MirInstrKind::ConstFloat(*v), expr.span()))
            }
            TypedExprHir::Str(v, _) => fb.emit(MirInstr::new(
                MirInstrKind::ConstStr(v.clone()),
                expr.span(),
            )),
            TypedExprHir::Bool(true, _) => fb.emit(MirInstr::new(MirInstrKind::True, expr.span())),
            TypedExprHir::Bool(false, _) => {
                fb.emit(MirInstr::new(MirInstrKind::False, expr.span()))
            }
            TypedExprHir::Ident {
                binding_id,
                name,
                access,
                span,
                ..
            } => {
                if self.is_function_binding(*binding_id, name) {
                    match self.lcx.func_ids.get(name) {
                        Some(&fid) => fb.emit(MirInstr::new(MirInstrKind::LoadFunc(fid), *span)),
                        None => {
                            return Err(LowerError::new(
                                LowerErrorKind::UndeclaredBinding { name: name.clone() },
                                *span,
                            ));
                        }
                    }
                } else {
                    match self.loc(*binding_id, name, *span)? {
                        Location::Local(slot) => match access {
                            Access::Copy => {
                                fb.emit(MirInstr::new(MirInstrKind::LoadLocal(slot), *span));
                            }
                            Access::Move => {
                                fb.emit(MirInstr::new(MirInstrKind::MoveLocal(slot), *span));
                            }
                            Access::Clone => {
                                fb.emit(MirInstr::new(MirInstrKind::CloneLocal(slot), *span));
                            }
                        },
                        Location::Global(gid) => {
                            match access {
                                Access::Copy => {
                                    fb.emit(MirInstr::new(
                                        MirInstrKind::LoadGlobal(gid.0 as u16),
                                        *span,
                                    ));
                                }
                                Access::Clone => {
                                    fb.emit(MirInstr::new(
                                        MirInstrKind::CloneGlobal(gid.0 as u16),
                                        *span,
                                    ));
                                }
                                Access::Move => {
                                    // Defensive: typeck rejects moving owned globals.
                                    return Err(LowerError::new(
                                        LowerErrorKind::UnsupportedFeature {
                                            feature: "move out of owned global",
                                        },
                                        *span,
                                    ));
                                }
                            }
                        }
                    }
                }
            }
            TypedExprHir::Assign {
                name,
                binding_id,
                rhs,
                span,
                ..
            } => {
                self.lower_expr_into(fb, rhs)?;
                // Dup/DupDeep for owned types (B2 decision).
                let dup_kind = if rhs.ty().is_owned() {
                    MirInstrKind::DupDeep
                } else {
                    MirInstrKind::Dup
                };
                fb.emit(MirInstr::new(dup_kind, *span));
                match self.loc(*binding_id, name, *span)? {
                    Location::Local(slot) => {
                        fb.emit(MirInstr::new(MirInstrKind::StoreLocal(slot), *span))
                    }
                    Location::Global(gid) => fb.emit(MirInstr::new(
                        MirInstrKind::StoreGlobal(gid.0 as u16),
                        *span,
                    )),
                }
            }
            TypedExprHir::Not(e) => {
                self.lower_expr_into(fb, e)?;
                fb.emit(MirInstr::new(MirInstrKind::Not, expr.span()));
            }
            TypedExprHir::Neg(e) => {
                self.lower_expr_into(fb, e)?;
                match e.ty() {
                    Type::Int => fb.emit(MirInstr::new(MirInstrKind::NegI, expr.span())),
                    Type::Float => fb.emit(MirInstr::new(MirInstrKind::NegF, expr.span())),
                    _ => {
                        return Err(LowerError::new(
                            LowerErrorKind::InvalidOperand { op: "neg" },
                            e.span(),
                        ));
                    }
                }
            }
            TypedExprHir::Add(l, r) => self.binop(fb, l, r, |int| {
                if int {
                    MirInstrKind::IAdd
                } else {
                    MirInstrKind::FAdd
                }
            })?,
            TypedExprHir::Sub(l, r) => self.binop(fb, l, r, |int| {
                if int {
                    MirInstrKind::ISub
                } else {
                    MirInstrKind::FSub
                }
            })?,
            TypedExprHir::Mul(l, r) => self.binop(fb, l, r, |int| {
                if int {
                    MirInstrKind::IMul
                } else {
                    MirInstrKind::FMul
                }
            })?,
            TypedExprHir::Div(l, r) => self.binop(fb, l, r, |int| {
                if int {
                    MirInstrKind::IDiv
                } else {
                    MirInstrKind::FDiv
                }
            })?,
            TypedExprHir::Mod(l, r) => {
                self.lower_expr_into(fb, l)?;
                self.lower_expr_into(fb, r)?;
                if matches!(l.ty(), Type::Int) && matches!(r.ty(), Type::Int) {
                    fb.emit(MirInstr::new(MirInstrKind::IMod, expr.span()));
                } else {
                    return Err(LowerError::new(
                        LowerErrorKind::InvalidOperand { op: "mod" },
                        expr.span(),
                    ));
                }
            }
            TypedExprHir::Eq(l, r)
            | TypedExprHir::Ne(l, r)
            | TypedExprHir::Lt(l, r)
            | TypedExprHir::Gt(l, r)
            | TypedExprHir::Le(l, r)
            | TypedExprHir::Ge(l, r) => {
                self.lower_expr_into(fb, l)?;
                self.lower_expr_into(fb, r)?;
                fb.emit(MirInstr::new(
                    match expr {
                        TypedExprHir::Eq(..) => MirInstrKind::Eq,
                        TypedExprHir::Ne(..) => MirInstrKind::Ne,
                        TypedExprHir::Lt(..) => MirInstrKind::Lt,
                        TypedExprHir::Gt(..) => MirInstrKind::Gt,
                        TypedExprHir::Le(..) => MirInstrKind::Le,
                        _ => MirInstrKind::Ge,
                    },
                    expr.span(),
                ));
            }
            TypedExprHir::And(l, r) => {
                // c0: l; JumpIfFalse F   c1: r; JumpIfFalse F   t: True   F: False
                // Layout: c0, c1, t, F, M
                self.lower_expr_into(fb, l)?;
                let c0 = fb.jump_if_false_later();
                let c1_b = fb.new_block();
                fb.start(c1_b);
                self.lower_expr_into(fb, r)?;
                let c1 = fb.jump_if_false_later();
                let t = fb.new_block();
                fb.start(t);
                fb.emit(MirInstr::new(MirInstrKind::True, expr.span()));
                let jt = fb.jump_later();
                let f_b = fb.new_block();
                fb.start(f_b);
                fb.emit(MirInstr::new(MirInstrKind::False, expr.span()));
                let jf = fb.jump_later();
                let m = fb.new_block();
                fb.start(m);
                fb.resolve(c0, f_b);
                fb.resolve(c1, f_b);
                fb.resolve(jt, m);
                fb.resolve(jf, m);
            }
            TypedExprHir::Or(l, r) => {
                // c0: l; JumpIfTrue T   c1: r; JumpIfTrue T   F: False   T: True
                // Layout: c0, c1, F, T, M
                self.lower_expr_into(fb, l)?;
                let c0 = fb.jump_if_true_later();
                let c1_b = fb.new_block();
                fb.start(c1_b);
                self.lower_expr_into(fb, r)?;
                let c1 = fb.jump_if_true_later();
                let f_b = fb.new_block();
                fb.start(f_b);
                fb.emit(MirInstr::new(MirInstrKind::False, expr.span()));
                let jf = fb.jump_later();
                let t = fb.new_block();
                fb.start(t);
                fb.emit(MirInstr::new(MirInstrKind::True, expr.span()));
                let jt = fb.jump_later();
                let m = fb.new_block();
                fb.start(m);
                fb.resolve(c0, t);
                fb.resolve(c1, t);
                fb.resolve(jf, m);
                fb.resolve(jt, m);
            }
            TypedExprHir::If(e) => self.lower_if(fb, e)?,
            TypedExprHir::While(e) => {
                // Dedicated head block: the back edge must re-enter only
                // "re-evaluate the condition", not the current merge block.
                // Otherwise the first entry (carrying the merge value of the
                // surrounding context on the stack) and a back-edge entry
                // (carrying the loop-carried depth) would disagree on stack
                // depth, and the verifier would (rightly) report
                // StackDepthMismatch.
                let head = fb.new_block();
                fb.jump(head);
                fb.start(head);
                self.lower_expr_into(fb, &e.condition)?;
                let c = fb.jump_if_false_later();
                let body = fb.new_block();
                fb.start(body);
                self.lower_block(fb, &e.body)?;
                fb.emit(MirInstr::new(MirInstrKind::Pop, e.span));
                fb.jump(head);
                let end = fb.new_block();
                fb.start(end);
                fb.emit(MirInstr::new(MirInstrKind::Unit, e.span));
                fb.resolve(c, end);
            }
            // 0.0.3 V04: `for` — desugared here (never in the parser: parse
            // emits a real `ExprFor` node, so no phase is skipped).
            TypedExprHir::For(e) => self.lower_for(fb, e)?,
            TypedExprHir::Choose(e) => self.lower_choose(fb, e)?,
            TypedExprHir::Call(callee, args, _) => {
                self.lower_call(fb, callee, args, expr.span())?;
            }
            TypedExprHir::Block(b) => self.lower_block(fb, b)?,
            TypedExprHir::Cast(inner, _) => {
                self.lower_expr_into(fb, inner)?;
                fb.emit(MirInstr::new(MirInstrKind::ToStr, expr.span()));
            }
            // 0.0.2 U06: `?` operator (Result propagation).
            TypedExprHir::Question { operand, span, .. } => {
                self.lower_question(fb, operand, *span)?;
            }
            // 0.0.4 T05: `e await`. Lower the operand (a `Future<T>` value) then
            // drive it to completion: `Await` pops the Future, resumes the
            // coroutine frame, and leaves the body's resulting `T` on the
            // stack (Δ0). `Future` is owned and non-Copy, so a single
            // drive is the only way to consume it (PLAN §3.3).
            TypedExprHir::Await { operand, span, .. } => {
                self.lower_expr_into(fb, operand)?;
                fb.emit(MirInstr::new(MirInstrKind::Await, *span));
            }
            // 0.0.4 T07: channel operations.
            TypedExprHir::ChannelNew {
                capacity,
                payload_ty,
                span,
            } => {
                self.lower_expr_into(fb, capacity)?;
                fb.emit(MirInstr::new(
                    MirInstrKind::ChannelNew(payload_ty.clone()),
                    *span,
                ));
            }
            TypedExprHir::ChannelSend {
                channel,
                value,
                payload_ty,
                span,
            } => {
                self.lower_expr_into(fb, channel)?;
                self.lower_expr_into(fb, value)?;
                fb.emit(MirInstr::new(
                    MirInstrKind::ChannelSend(payload_ty.clone()),
                    *span,
                ));
            }
            TypedExprHir::ChannelRecv {
                channel,
                payload_ty,
                span,
            } => {
                self.lower_expr_into(fb, channel)?;
                fb.emit(MirInstr::new(
                    MirInstrKind::ChannelRecv(payload_ty.clone()),
                    *span,
                ));
            }
            TypedExprHir::ChannelSplit { channel, span } => {
                self.lower_expr_into(fb, channel)?;
                fb.emit(MirInstr::new(MirInstrKind::ChannelSplit, *span));
            }
            // 0.0.2 U06: Result constructors.
            TypedExprHir::ResultCtor {
                ctor, value, span, ..
            } => {
                self.lower_expr_into(fb, value)?;
                let kind = match ctor {
                    ResultCtor::Ok => MirInstrKind::PackOk,
                    ResultCtor::Err => MirInstrKind::PackErr,
                };
                fb.emit(MirInstr::new(kind, *span));
            }
            // 0.0.2 U06: box expression.
            TypedExprHir::Box(inner, span) => {
                self.lower_expr_into(fb, inner)?;
                fb.emit(MirInstr::new(MirInstrKind::AllocBox, *span));
            }
            // 0.0.2 U06: deref expression (read).
            TypedExprHir::Deref(inner, span) => {
                self.lower_expr_into(fb, inner)?;
                fb.emit(MirInstr::new(MirInstrKind::DerefBox, *span));
                // `DerefBox` is `b → b v` (BYTECODE.md §5.9): the box stays on
                // the stack and the pointee copy is pushed on top, so a bare
                // read leaves TWO values where an expression must leave one.
                // Park the copy in a temp slot, drop the box, then reload the
                // copy. This keeps `DerefBox`'s frozen stack effect (Δ+1) and
                // still yields exactly one value.
                let tmp = self.fresh_temp();
                fb.emit(MirInstr::new(MirInstrKind::StoreLocal(tmp), *span));
                fb.emit(MirInstr::new(MirInstrKind::Pop, *span));
                fb.emit(MirInstr::new(MirInstrKind::LoadLocal(tmp), *span));
            }
            // 0.0.5 T02: the pointer surface is fully typed by typeck, but
            // its bytecode encoding (`AddrOf` 0xC8 / `LoadPtr` 0xC9 /
            // `StorePtr` 0xCA, `.flnc` version 5) lands with T05 — a
            // ptr program is rejected here at the correct phase, never
            // mis-lowered.
            // 0.0.5 T05: `addr <place>` — take the address of a place.
            // Only `Ident` places (local/global bindings) are encodable now;
            // struct fields, array elements and box interiors need the VM's
            // pointer table representation, which lands with T06 — reject
            // them here so no mis-lowered ptr program escapes this phase.
            TypedExprHir::Addr { place, span, .. } => match &**place {
                TypedExprHir::Ident {
                    name, binding_id, ..
                } => {
                    let place_id = match self.loc(*binding_id, name, *span)? {
                        Location::Local(slot) => slot,
                        Location::Global(gid) => gid.0 as u16 | 0x8000,
                    };
                    fb.emit(MirInstr::new(MirInstrKind::AddrOf(place_id), *span));
                }
                _other => {
                    return Err(LowerError::new(
                        LowerErrorKind::UnsupportedFeature {
                            feature: "unsupported addr place kind",
                        },
                        *span,
                    ));
                }
            },
            // `deref_ptr <p>`: push the pointer, then read through it.
            // `LoadPtr` is `ptr → v` (Δ0, min 1): pops the pointer and pushes
            // a copy of the pointee; the pointer itself is untouched.
            TypedExprHir::DerefPtr(inner, span) => {
                self.lower_expr_into(fb, inner)?;
                fb.emit(MirInstr::new(MirInstrKind::LoadPtr, *span));
            }
            // `deref_ptr <p> = <rhs>`: push the value, then the pointer,
            // then overwrite the pointee. `StorePtr` is `v ptr →` (Δ−2, min 2);
            // the assignment expression yields `unit`.
            TypedExprHir::AssignDerefPtr {
                name,
                binding_id,
                rhs,
                span,
                ..
            } => {
                self.lower_expr_into(fb, rhs)?;
                // Defensive only: typeck's `PtrNotStorable` seals global ptr
                // declarations (`typeck_assign_deref_ptr`), so the pointer
                // binding is always a local — the guard exists so a future
                // typeck change cannot let a mis-lowered store escape here.
                let Location::Local(slot) = self.loc(*binding_id, name, *span)? else {
                    return Err(LowerError::new(
                        LowerErrorKind::UnsupportedFeature {
                            feature: "write through a pointer bound in a global (not storable)",
                        },
                        *span,
                    ));
                };
                fb.emit(MirInstr::new(MirInstrKind::LoadLocal(slot), *span));
                fb.emit(MirInstr::new(MirInstrKind::StorePtr, *span));
                fb.emit(MirInstr::new(MirInstrKind::Unit, *span));
            }
            // 0.0.2 U06: deref assignment.
            TypedExprHir::AssignDeref {
                binding_id,
                rhs,
                span,
                ..
            } => {
                let box_loc = self.loc(*binding_id, "?", *span)?;
                match box_loc {
                    Location::Local(slot) => {
                        // 0.0.2 U09 fix: `StoreDerefBox`'s contract pops the
                        // box and discards it, and `LoadLocal` yields a deep
                        // copy — a write through the loaded copy could never
                        // reach the box in the slot (the same copy-through
                        // limitation that makes global deref-assign a
                        // typeck error). Under unique ownership (§10.3: no
                        // handle may be stored) replacing the pointee is
                        // observationally identical to replacing the box:
                        // evaluate rhs first (the old box is still in the
                        // slot, so a self-referential rhs reads the old
                        // value), box it, and let `StoreLocal` overwrite the
                        // slot — the old box, and with it the old pointee,
                        // is released exactly there.
                        self.lower_expr_into(fb, rhs)?;
                        fb.emit(MirInstr::new(MirInstrKind::AllocBox, *span));
                        fb.emit(MirInstr::new(MirInstrKind::StoreLocal(slot), *span));
                        fb.emit(MirInstr::new(MirInstrKind::Unit, *span));
                    }
                    Location::Global(gid) => {
                        return Err(LowerError::new(
                            LowerErrorKind::GlobalBoxDerefAssign {
                                name: format!("global#{}", gid.0),
                            },
                            *span,
                        ));
                    }
                }
            }
            // 0.0.3 V02: struct literal. StructNew takes the fields in
            // *declaration* order; the initializers keep *source* order
            // (typeck pairs each with its declaration index), so when the
            // two differ the values are parked in temp slots and reloaded
            // in declaration order — source-order side effects, correct
            // field order.
            TypedExprHir::StructLit {
                struct_id,
                inits,
                span,
                ..
            } => {
                let in_decl_order = inits_in_decl_order(inits);
                if in_decl_order {
                    for (_, init) in inits {
                        self.lower_expr_into(fb, init)?;
                    }
                } else {
                    let mut order: Vec<usize> = (0..inits.len()).collect();
                    order.sort_by_key(|&i| inits[i].0);
                    let mut temps = Vec::with_capacity(inits.len());
                    for (_, init) in inits {
                        self.lower_expr_into(fb, init)?;
                        let tmp = self.fresh_temp();
                        fb.emit(MirInstr::new(MirInstrKind::StoreLocal(tmp), *span));
                        temps.push(tmp);
                    }
                    for i in order {
                        let tmp = temps[i];
                        fb.emit(MirInstr::new(MirInstrKind::LoadLocal(tmp), *span));
                    }
                }
                fb.emit(MirInstr::new(
                    MirInstrKind::StructNew(struct_id.0 as u16),
                    *span,
                ));
            }
            // 0.0.3 V02: field read — `GetField` pops the object (a fresh
            // copy: the load above cloned the binding, and a borrow handle
            // is dereferenced by the VM) and pushes the field copy.
            TypedExprHir::Field {
                obj,
                field_idx,
                span,
                ..
            } => {
                self.lower_expr_into(fb, obj)?;
                fb.emit(MirInstr::new(MirInstrKind::GetField(*field_idx), *span));
            }
            // 0.0.3 V02: one-level field write. Mirrors `Assign`: evaluate
            // the RHS, duplicate it (the assignment expression's value),
            // then store into the field — the old field value is released
            // by the overwrite.
            TypedExprHir::AssignField {
                obj_binding_id,
                field_idx,
                rhs,
                span,
                ..
            } => {
                self.lower_expr_into(fb, rhs)?;
                let dup_kind = if rhs.ty().is_owned() {
                    MirInstrKind::DupDeep
                } else {
                    MirInstrKind::Dup
                };
                fb.emit(MirInstr::new(dup_kind, *span));
                match self.loc(*obj_binding_id, "?", *span)? {
                    Location::Local(slot) => fb.emit(MirInstr::new(
                        MirInstrKind::SetFieldLocal(slot, *field_idx),
                        *span,
                    )),
                    Location::Global(gid) => fb.emit(MirInstr::new(
                        MirInstrKind::SetFieldGlobal(gid.0 as u16, *field_idx),
                        *span,
                    )),
                }
            }
            // 0.0.3 V03: array literal. Elements evaluate in source order
            // (which is the declaration order for arrays — no temp-slot
            // reorder, unlike struct literals).
            TypedExprHir::ArrayLit { elems, span, .. } => {
                let n = arr_len_u16(elems.len(), *span)?;
                for e in elems {
                    self.lower_expr_into(fb, e)?;
                }
                fb.emit(MirInstr::new(MirInstrKind::ArrNew(n), *span));
            }
            // 0.0.3 V03: element read — bounds-checked copy (D3). The array
            // operand is a fresh copy (the load above cloned the binding,
            // and a borrow handle is dereferenced by the VM), so the read
            // never disturbs the original.
            TypedExprHir::Index { arr, idx, span, .. } => {
                self.lower_expr_into(fb, arr)?;
                self.lower_expr_into(fb, idx)?;
                fb.emit(MirInstr::new(MirInstrKind::ArrGet, *span));
            }
            // 0.0.3 V03: one-level element write. Mirrors `AssignField`:
            // evaluate the RHS, duplicate it (the assignment expression's
            // value), then evaluate the index and store — the old element
            // is released by the overwrite. Stack shape `v i →` leaves the
            // duplicated value as the expression's value.
            TypedExprHir::AssignIndex {
                binding_id,
                index,
                rhs,
                span,
                ..
            } => {
                self.lower_expr_into(fb, rhs)?;
                let dup_kind = if rhs.ty().is_owned() {
                    MirInstrKind::DupDeep
                } else {
                    MirInstrKind::Dup
                };
                fb.emit(MirInstr::new(dup_kind, *span));
                self.lower_expr_into(fb, index)?;
                match self.loc(*binding_id, "?", *span)? {
                    Location::Local(slot) => {
                        fb.emit(MirInstr::new(MirInstrKind::ArrSetLocal(slot), *span))
                    }
                    Location::Global(gid) => fb.emit(MirInstr::new(
                        MirInstrKind::ArrSetGlobal(gid.0 as u16),
                        *span,
                    )),
                }
            }
        }
        Ok(())
    }

    /// Lower `l op r`; `pick(is_int)` selects the instruction.
    fn binop(
        &mut self,
        fb: &mut FnBuilder,
        l: &TypedExprHir,
        r: &TypedExprHir,
        pick: impl Fn(bool) -> MirInstrKind,
    ) -> Result<(), LowerError> {
        self.lower_expr_into(fb, l)?;
        self.lower_expr_into(fb, r)?;
        fb.emit(MirInstr::new(pick(matches!(l.ty(), Type::Int)), l.span()));
        Ok(())
    }

    /// Lower a function call with ref argument handling.
    fn lower_call(
        &mut self,
        fb: &mut FnBuilder,
        callee: &TypedExprHir,
        args: &[TypedExprHir],
        span: Span,
    ) -> Result<(), LowerError> {
        match callee {
            TypedExprHir::Ident {
                name, binding_id, ..
            } if self.is_function_binding(*binding_id, name) => {
                let mut fid = self.lcx.func_ids[name];
                // Builtins may collapse to something other than a runtime
                // call. The `is_builtin` guard keeps a user function that
                // shadows the name a plain call.
                if self.lcx.funcs[fid.0 as usize].is_builtin
                    && let Some(sig) = find_builtin(name)
                {
                    match sig.lowering {
                        crate::typeck::builtins::BuiltinLowering::InlineIter => {
                            // 0.0.3 V07: `std.collection.iter.map/filter/
                            // fold/take` expand inline into their loop (F4:
                            // eager library functions, no runtime calls).
                            return self.lower_inline_iter(fb, &sig, args, span);
                        }
                        crate::typeck::builtins::BuiltinLowering::MakeRange => {
                            // `range`/`range_from` (D5) are table-recognized
                            // Range constructors: the call collapses to
                            // `MakeRange` instead of a runtime call — the
                            // `Ok`/`Err` mechanism (a recognized name
                            // lowering to a dedicated instruction) extended
                            // to the builtin table (0.0.3 V06).
                            // Typeck guarantees exactly the signature's
                            // arity of `int` arguments. `MakeRange` pops
                            // `start end`; a bare `range(n)` is `0..n` (D5),
                            // so its start is pushed first —
                            // `range_from(a, b)` supplies both bounds.
                            if let crate::typeck::builtins::BuiltinParam::Fixed(p) = &sig.param
                                && p.len() == 1
                            {
                                fb.emit(MirInstr::new(MirInstrKind::ConstInt(0), span));
                            }
                            for arg in args {
                                self.lower_expr_into(fb, arg)?;
                            }
                            fb.emit(MirInstr::new(MirInstrKind::MakeRange, span));
                            return Ok(());
                        }
                        crate::typeck::builtins::BuiltinLowering::Intrinsic => {}
                    }
                }
                // Variadic builtins: resolve to (or instantiate) the
                // instance whose `params` matches this call's argc.
                if self.lcx.funcs[fid.0 as usize].is_builtin
                    && self.lcx.funcs[fid.0 as usize].params != args.len() as u16
                {
                    fid = self.lcx.variadic_builtin_fid(name, args.len() as u16);
                }
                let param_tys = self.lcx.param_types.get(&fid).cloned().unwrap_or_default();
                self.prepare_args(fb, args, &param_tys)?;
                fb.emit(MirInstr::new(MirInstrKind::Call(fid), span));
                // 0.0.4 T06: suspend-source builtins lower to a trailing
                // `Suspend` so the task parks in the cooperative scheduler.
                // `std.io.sleep` (deadline recorded by the VM's builtin) and
                // `std.async.yield_now` (no deadline ⇒ immediately re-runnable)
                // detect, in an async body, park here. `std.async.run` also
                // suspends when called from inside an async body so it can wait
                // for the spawned tasks to complete before continuing. A sync
                // caller never emits the `Suspend`, so these are no-ops there.
                if self.is_async
                    && matches!(
                        name.as_str(),
                        "std.io.sleep" | "std.async.yield_now" | "std.async.run"
                    )
                {
                    fb.emit(MirInstr::new(MirInstrKind::Suspend, span));
                }
            }
            _ => {
                // Indirect call: get param types from callee's function type.
                self.lower_expr_into(fb, callee)?;
                let param_tys = match callee.ty() {
                    Type::Func(params, _) => params,
                    _ => vec![],
                };
                self.prepare_args(fb, args, &param_tys)?;
                fb.emit(MirInstr::new(
                    MirInstrKind::CallValue(args.len() as u8),
                    span,
                ));
            }
        }
        Ok(())
    }

    /// Prepare call arguments, handling ref parameter places.
    fn prepare_args(
        &mut self,
        fb: &mut FnBuilder,
        args: &[TypedExprHir],
        param_tys: &[Type],
    ) -> Result<(), LowerError> {
        for (i, arg) in args.iter().enumerate() {
            let param_ty = param_tys.get(i);
            if param_ty.is_some_and(|t| matches!(t, Type::Ref(_))) {
                // Ref parameter: argument must be a bare Ident place (typeck guarantee).
                match arg {
                    TypedExprHir::Ident {
                        binding_id,
                        name,
                        span,
                        ..
                    } => {
                        match self.loc(*binding_id, name, *span)? {
                            Location::Local(slot) => {
                                fb.emit(MirInstr::new(MirInstrKind::MakeRefLocal(slot), *span));
                            }
                            Location::Global(gid) => {
                                // Global ref arg: copy to temp slot, then borrow the temp.
                                let tmp = self.fresh_temp();
                                fb.emit(MirInstr::new(
                                    MirInstrKind::CloneGlobal(gid.0 as u16),
                                    *span,
                                ));
                                fb.emit(MirInstr::new(MirInstrKind::StoreLocal(tmp), *span));
                                fb.emit(MirInstr::new(MirInstrKind::MakeRefLocal(tmp), *span));
                            }
                        }
                    }
                    _ => {
                        // Defensive: typeck should reject non-place ref args.
                        return Err(LowerError::new(
                            LowerErrorKind::UnsupportedFeature {
                                feature: "ref argument must be a variable (typeck should reject)",
                            },
                            arg.span(),
                        ));
                    }
                }
            } else {
                // Normal argument: lower and push.
                self.lower_expr_into(fb, arg)?;
            }
        }
        Ok(())
    }

    /// Lower `?` operator (Result propagation).
    ///
    /// Layout (BYTECODE.md §6.7):
    /// ```text
    /// <eval operand> Dup/DupDeep  IsErr  JumpIfTrue L_err
    /// L_ok: UnwrapOk  Jump L_cont
    /// L_err: UnwrapErr  PackErr  Return
    /// L_cont: ...
    /// ```
    /// The ok-path is a distinct block from the operand block so the
    /// unconditional `Jump L_cont` does not overwrite the conditional
    /// `JumpIfTrue` terminator.
    fn lower_question(
        &mut self,
        fb: &mut FnBuilder,
        operand: &TypedExprHir,
        span: Span,
    ) -> Result<(), LowerError> {
        self.lower_expr_into(fb, operand)?;
        // Dup/DupDeep based on operand type (owned → DupDeep).
        let dup_kind = if operand.ty().is_owned() {
            MirInstrKind::DupDeep
        } else {
            MirInstrKind::Dup
        };
        fb.emit(MirInstr::new(dup_kind, span));
        fb.emit(MirInstr::new(MirInstrKind::IsErr, span));
        let c = fb.jump_if_true_later();
        // Ok path: unwrap and fall through to the continuation.
        let ok = fb.new_block();
        fb.start(ok);
        fb.emit(MirInstr::new(MirInstrKind::UnwrapOk, span));
        let j_cont = fb.jump_later();
        // Err path: re-pack the error (must nest it, not return it bare) and
        // return from the enclosing function.
        let l_err = fb.new_block();
        fb.start(l_err);
        fb.emit(MirInstr::new(MirInstrKind::UnwrapErr, span));
        fb.emit(MirInstr::new(MirInstrKind::PackErr, span));
        fb.end(Terminator::Return);
        let l_cont = fb.new_block();
        fb.start(l_cont);
        fb.resolve(c, l_err);
        fb.resolve(j_cont, l_cont);
        Ok(())
    }

    /// Lower `for x in <iterable> { body }` (0.0.3 V04, PLAN §5.4).
    ///
    /// ```text
    /// <eval iterable>; StoreLocal __it      ; evaluated once (DESIGN.md §6)
    /// __i = <start>; __count = <bound>      ; hoisted out of the loop
    /// L_head: LoadLocal __i; LoadLocal __count; Lt; JumpIfFalse L_end
    /// L_body: x = <element copy>; <body>; Pop
    ///         LoadLocal __i; Const 1; IAdd; StoreLocal __i; Jump L_head
    /// L_end:  Unit
    /// ```
    ///
    /// The head block is dedicated (exactly like `while`) so that the back
    /// edge re-enters only the bound check — otherwise the first entry
    /// (carrying the merge value of the surrounding context) and a back-edge
    /// entry would disagree on stack depth and the verifier would rightly
    /// report `StackDepthMismatch`.
    fn lower_for(&mut self, fb: &mut FnBuilder, e: &TypedExprForHir) -> Result<(), LowerError> {
        // Synthetic slots are allocated by *number* (`fresh`), never by name,
        // so a user binding can never collide with them — the `__`-prefixed
        // names in the comments below are documentation only.
        let it = self.fresh_temp();
        let idx = self.fresh_temp();
        let count = self.fresh_temp();

        let kind = IterKind::of(&e.iterable).ok_or_else(|| {
            LowerError::new(
                LowerErrorKind::UnsupportedFeature {
                    feature: "for over a type that is neither array nor Range \
                              (typeck should reject)",
                },
                e.span,
            )
        })?;

        // The iterable is evaluated once, before the loop (DESIGN.md §6).
        self.lower_expr_into(fb, &e.iterable)?;
        fb.emit(MirInstr::new(MirInstrKind::StoreLocal(it), e.span));

        // `__i` starts at the first element and `__count` holds the exclusive
        // end. `ArrLen` / `RangeStart` / `RangeEnd` are peek-shaped
        // (`a → a int`): the operands they read stay on the stack, so the
        // parked copy of the iterable is dropped with a `Pop` after each
        // bound is stored.
        match kind {
            IterKind::Array => {
                fb.emit(MirInstr::new(MirInstrKind::LoadLocal(it), e.span));
                fb.emit(MirInstr::new(MirInstrKind::ArrLen, e.span));
                fb.emit(MirInstr::new(MirInstrKind::StoreLocal(count), e.span));
                fb.emit(MirInstr::new(MirInstrKind::Pop, e.span));
                fb.emit(MirInstr::new(MirInstrKind::ConstInt(0), e.span));
                fb.emit(MirInstr::new(MirInstrKind::StoreLocal(idx), e.span));
            }
            IterKind::Range => {
                fb.emit(MirInstr::new(MirInstrKind::LoadLocal(it), e.span));
                fb.emit(MirInstr::new(MirInstrKind::RangeStart, e.span));
                fb.emit(MirInstr::new(MirInstrKind::StoreLocal(idx), e.span));
                fb.emit(MirInstr::new(MirInstrKind::Pop, e.span));
                fb.emit(MirInstr::new(MirInstrKind::LoadLocal(it), e.span));
                fb.emit(MirInstr::new(MirInstrKind::RangeEnd, e.span));
                fb.emit(MirInstr::new(MirInstrKind::StoreLocal(count), e.span));
                fb.emit(MirInstr::new(MirInstrKind::Pop, e.span));
            }
        }

        // The loop variable is a fresh immutable binding per iteration;
        // `resolve` already gave it a `BindingId`, which owns its slot.
        let var_slot = self.slots.slot_for(e.var_binding_id);

        let head = fb.new_block();
        fb.jump(head);
        fb.start(head);
        fb.emit(MirInstr::new(MirInstrKind::LoadLocal(idx), e.span));
        fb.emit(MirInstr::new(MirInstrKind::LoadLocal(count), e.span));
        fb.emit(MirInstr::new(MirInstrKind::Lt, e.span));
        let cond = fb.jump_if_false_later();

        let body = fb.new_block();
        fb.start(body);
        // Element copy: `ArrGet` copies (deep for owned elements, D3), so the
        // iteration never consumes the iterable.
        match kind {
            IterKind::Array => {
                fb.emit(MirInstr::new(MirInstrKind::LoadLocal(it), e.span));
                fb.emit(MirInstr::new(MirInstrKind::LoadLocal(idx), e.span));
                fb.emit(MirInstr::new(MirInstrKind::ArrGet, e.span));
            }
            IterKind::Range => {
                fb.emit(MirInstr::new(MirInstrKind::LoadLocal(idx), e.span));
            }
        }
        fb.emit(MirInstr::new(MirInstrKind::StoreLocal(var_slot), e.span));
        self.lower_block(fb, &e.body)?;
        // The body's value (its tail expression) is discarded: `for` is a
        // statement whose value is `unit`.
        fb.emit(MirInstr::new(MirInstrKind::Pop, e.span));
        fb.emit(MirInstr::new(MirInstrKind::LoadLocal(idx), e.span));
        fb.emit(MirInstr::new(MirInstrKind::ConstInt(1), e.span));
        fb.emit(MirInstr::new(MirInstrKind::IAdd, e.span));
        fb.emit(MirInstr::new(MirInstrKind::StoreLocal(idx), e.span));
        fb.jump(head);

        let end = fb.new_block();
        fb.start(end);
        fb.emit(MirInstr::new(MirInstrKind::Unit, e.span));
        fb.resolve(cond, end);
        Ok(())
    }

    /// Expand a `std.collection.iter` inline call (0.0.3 V07, F4).
    ///
    /// The four schematic helpers lower (in the compiler, with no runtime
    /// iterator objects — PLAN §3.5.2) to the loop their eager semantics
    /// imply: accumulate into a fresh array via the always-registered
    /// `std.arr.push` builtin (`map`/`filter`/`take`) or a single
    /// accumulator (`fold`). Arguments are evaluated once, left to right,
    /// into fresh temp slots (D4 discipline), so a `map(xs, f)` call does
    /// not re-evaluate `xs` or `f` per iteration.
    ///
    /// Typechecking is schematic (two type variables for `map`/`fold`); the
    /// MIR emitted here is untyped, exactly like `lower_for`.
    ///
    /// The loop shape mirrors `lower_for`: a head block re-entered only by
    /// the increment back-edge so entry and back-edge agree on stack depth
    /// (the V04 standing fix). The result (`[U]`/`[T]`/`A`) is left on the
    /// stack at the `end` block.
    fn lower_inline_iter(
        &mut self,
        fb: &mut FnBuilder,
        sig: &crate::typeck::builtins::BuiltinSig,
        args: &[TypedExprHir],
        span: Span,
    ) -> Result<(), LowerError> {
        let kind = InlineIterKind::of(sig.name).ok_or_else(|| {
            LowerError::new(
                LowerErrorKind::UnsupportedFeature {
                    feature: "unknown std.collection.iter inline helper",
                },
                span,
            )
        })?;

        // `std.arr.push` is a builtin, always mirrored into `func_ids` by
        // `LowerCtx::new` (the whole table), so the accumulator can call it
        // without the user importing `std.arr`. A missing entry is a
        // compiler-invariant break, reported rather than panicked.
        let push_fid = *self.lcx.func_ids.get("std.arr.push").ok_or_else(|| {
            LowerError::new(
                LowerErrorKind::UnsupportedFeature {
                    feature: "std.arr.push builtin not registered",
                },
                span,
            )
        })?;

        // Evaluate every argument once, left to right, into fresh slots.
        let mut slots = Vec::with_capacity(args.len());
        for arg in args {
            self.lower_expr_into(fb, arg)?;
            let slot = self.fresh_temp();
            fb.emit(MirInstr::new(MirInstrKind::StoreLocal(slot), span));
            slots.push(slot);
        }
        let xs = slots[0];
        // The function value's slot: `map`/`filter` second arg, `fold`
        // third; `take` has none (its second arg is the count `n`).
        let f = kind.fn_arg_index().map(|i| slots[i]);
        let n = matches!(kind, InlineIterKind::Take).then(|| slots[1]);

        // `__count = len(xs)` — `ArrLen` is peek-shaped (`a → a int`), so
        // the parked array copy is dropped with a `Pop` (the `lower_for`
        // pattern).
        let count = self.fresh_temp();
        fb.emit(MirInstr::new(MirInstrKind::LoadLocal(xs), span));
        fb.emit(MirInstr::new(MirInstrKind::ArrLen, span));
        fb.emit(MirInstr::new(MirInstrKind::StoreLocal(count), span));
        fb.emit(MirInstr::new(MirInstrKind::Pop, span));

        // `take`: cap the count at `n` (`__count = min(__count, __n)`).
        // Shape: `n < count ? (__count = n) : ()`, with a shared merge.
        if let Some(n) = n {
            fb.emit(MirInstr::new(MirInstrKind::LoadLocal(n), span));
            fb.emit(MirInstr::new(MirInstrKind::LoadLocal(count), span));
            fb.emit(MirInstr::new(MirInstrKind::Lt, span));
            let cap_true = fb.jump_if_true_later();
            // `n < count` is false: skip the cap, fall through.
            let skip = fb.new_block();
            fb.start(skip);
            let skip_jump = fb.jump_later();
            // `n < count` is true: cap, then merge.
            let cap = fb.new_block();
            fb.start(cap);
            fb.emit(MirInstr::new(MirInstrKind::LoadLocal(n), span));
            fb.emit(MirInstr::new(MirInstrKind::StoreLocal(count), span));
            let cap_jump = fb.jump_later();
            let merge = fb.new_block();
            fb.start(merge);
            fb.resolve(cap_true, cap);
            fb.resolve(skip_jump, merge);
            fb.resolve(cap_jump, merge);
            // Continue emitting into the merge block below.
        }

        // `__out = []` for the array-building helpers; a fold uses the
        // pre-stored `init` slot as its accumulator.
        let out = if kind.is_fold() {
            slots[1]
        } else {
            let o = self.fresh_temp();
            fb.emit(MirInstr::new(MirInstrKind::ArrNew(0), span));
            fb.emit(MirInstr::new(MirInstrKind::StoreLocal(o), span));
            o
        };

        // `__i` — 0 counting up for the forward helpers; `count - 1`
        // counting down for `foldr` (0.0.6 T07). An empty array yields
        // `__i = -1`, which fails the head check immediately — no special
        // case needed (int slots are signed i64).
        let i = self.fresh_temp();
        if kind.is_foldr() {
            fb.emit(MirInstr::new(MirInstrKind::LoadLocal(count), span));
            fb.emit(MirInstr::new(MirInstrKind::ConstInt(1), span));
            fb.emit(MirInstr::new(MirInstrKind::ISub, span));
        } else {
            fb.emit(MirInstr::new(MirInstrKind::ConstInt(0), span));
        }
        fb.emit(MirInstr::new(MirInstrKind::StoreLocal(i), span));

        // Loop: head checks `__i < __count` (forward) or `0 <= __i`
        // (backward — the `Le` operand order is `a <= b`, so the constant
        // goes first).
        let head = fb.new_block();
        fb.jump(head);
        fb.start(head);
        if kind.is_foldr() {
            fb.emit(MirInstr::new(MirInstrKind::ConstInt(0), span));
            fb.emit(MirInstr::new(MirInstrKind::LoadLocal(i), span));
            fb.emit(MirInstr::new(MirInstrKind::Le, span));
        } else {
            fb.emit(MirInstr::new(MirInstrKind::LoadLocal(i), span));
            fb.emit(MirInstr::new(MirInstrKind::LoadLocal(count), span));
            fb.emit(MirInstr::new(MirInstrKind::Lt, span));
        }
        let cond_false = fb.jump_if_false_later();

        let body = fb.new_block();
        fb.start(body);

        // Element read is a bounds-checked copy (`ArrGet`, D3/D4), so the
        // source array is never consumed — and owned elements are deep-copied
        // before crossing a call boundary.
        let elem = self.fresh_temp();
        fb.emit(MirInstr::new(MirInstrKind::LoadLocal(xs), span));
        fb.emit(MirInstr::new(MirInstrKind::LoadLocal(i), span));
        fb.emit(MirInstr::new(MirInstrKind::ArrGet, span));
        fb.emit(MirInstr::new(MirInstrKind::StoreLocal(elem), span));

        match kind {
            InlineIterKind::Map => {
                // `mapped = f(elem)`; push it to `__out`.
                let fs = f.ok_or_else(|| self.iter_missing_fn(span))?;
                let mapped = self.fresh_temp();
                self.emit_indirect_call(fb, fs, &[elem], span);
                fb.emit(MirInstr::new(MirInstrKind::StoreLocal(mapped), span));
                self.emit_push_fold(fb, out, mapped, push_fid, span);
                self.emit_loop_increment(fb, i, head, span);
            }
            InlineIterKind::Filter => {
                // `keep = f(elem)`; push `elem` to `__out` only when true.
                let fs = f.ok_or_else(|| self.iter_missing_fn(span))?;
                self.emit_indirect_call(fb, fs, &[elem], span);
                let keep_false = fb.jump_if_false_later();
                // kept arm (fall-through of `keep`): `__out = push(__out, elem)`.
                let kept = fb.new_block();
                fb.start(kept);
                self.emit_push_fold(fb, out, elem, push_fid, span);
                let kept_jump = fb.jump_later();
                // dropped arm (empty), then both arms converge on the increment.
                let dropped = fb.new_block();
                fb.start(dropped);
                let dropped_jump = fb.jump_later();
                let inc = fb.new_block();
                fb.start(inc);
                fb.resolve(keep_false, dropped);
                fb.resolve(kept_jump, inc);
                fb.resolve(dropped_jump, inc);
                self.emit_loop_increment(fb, i, head, span);
            }
            InlineIterKind::Fold => {
                // `acc = f(acc, elem)`.
                let fs = f.ok_or_else(|| self.iter_missing_fn(span))?;
                let acc = out;
                self.emit_indirect_call(fb, fs, &[acc, elem], span);
                fb.emit(MirInstr::new(MirInstrKind::StoreLocal(acc), span));
                self.emit_loop_increment(fb, i, head, span);
            }
            InlineIterKind::FoldR => {
                // `acc = f(elem, acc)` — the T/A argument order reversed
                // (D5); the loop index is decremented toward -1.
                let fs = f.ok_or_else(|| self.iter_missing_fn(span))?;
                let acc = out;
                self.emit_indirect_call(fb, fs, &[elem, acc], span);
                fb.emit(MirInstr::new(MirInstrKind::StoreLocal(acc), span));
                self.emit_loop_decrement(fb, i, head, span);
            }
            InlineIterKind::Take => {
                // pass the element through unchanged.
                self.emit_push_fold(fb, out, elem, push_fid, span);
                self.emit_loop_increment(fb, i, head, span);
            }
        }

        let end = fb.new_block();
        fb.start(end);
        fb.resolve(cond_false, end);
        // The loop's result: `__out` for array builders, `__acc` for fold.
        fb.emit(MirInstr::new(MirInstrKind::LoadLocal(out), span));
        Ok(())
    }

    /// Emit `<f(a1, …, an)>` leaving the result on the stack
    /// (`LoadFunc`-independent: `f` is a pre-stored function value).
    fn emit_indirect_call(
        &mut self,
        fb: &mut FnBuilder,
        f_slot: u16,
        arg_slots: &[u16],
        span: Span,
    ) {
        fb.emit(MirInstr::new(MirInstrKind::LoadLocal(f_slot), span));
        for a in arg_slots {
            fb.emit(MirInstr::new(MirInstrKind::LoadLocal(*a), span));
        }
        fb.emit(MirInstr::new(
            MirInstrKind::CallValue(arg_slots.len() as u8),
            span,
        ));
    }

    /// Emit `__acc = push(__acc, value_slot)`, rebinding the accumulator.
    fn emit_push_fold(
        &mut self,
        fb: &mut FnBuilder,
        acc: u16,
        value_slot: u16,
        push_fid: FuncId,
        span: Span,
    ) {
        fb.emit(MirInstr::new(MirInstrKind::LoadLocal(acc), span));
        fb.emit(MirInstr::new(MirInstrKind::LoadLocal(value_slot), span));
        fb.emit(MirInstr::new(MirInstrKind::Call(push_fid), span));
        fb.emit(MirInstr::new(MirInstrKind::StoreLocal(acc), span));
    }

    /// Emit `__i = __i + 1` and jump back to the head.
    fn emit_loop_increment(&mut self, fb: &mut FnBuilder, i: u16, head: BlockId, span: Span) {
        fb.emit(MirInstr::new(MirInstrKind::LoadLocal(i), span));
        fb.emit(MirInstr::new(MirInstrKind::ConstInt(1), span));
        fb.emit(MirInstr::new(MirInstrKind::IAdd, span));
        fb.emit(MirInstr::new(MirInstrKind::StoreLocal(i), span));
        fb.jump(head);
    }

    /// `foldr`'s backward step: `__i -= 1`, then re-enter the head block
    /// (0.0.6 T07). The head checks `0 <= __i`, so the loop ends after the
    /// index passes the array start.
    fn emit_loop_decrement(&mut self, fb: &mut FnBuilder, i: u16, head: BlockId, span: Span) {
        fb.emit(MirInstr::new(MirInstrKind::LoadLocal(i), span));
        fb.emit(MirInstr::new(MirInstrKind::ConstInt(1), span));
        fb.emit(MirInstr::new(MirInstrKind::ISub, span));
        fb.emit(MirInstr::new(MirInstrKind::StoreLocal(i), span));
        fb.jump(head);
    }

    /// Defensive: an inline helper that needs a function argument was called
    /// without one (a typeck-invariant break; typeck fixes the arity).
    fn iter_missing_fn(&self, span: Span) -> LowerError {
        LowerError::new(
            LowerErrorKind::UnsupportedFeature {
                feature: "std.collection.iter helper missing its function argument",
            },
            span,
        )
    }

    fn lower_if(&mut self, fb: &mut FnBuilder, e: &TypedExprIfHir) -> Result<(), LowerError> {
        // Layout: c0; then0; c1; then1; …; else-or-unit; M
        let mut cond_blocks: Vec<BlockId> = Vec::new();
        let mut branch_jumps: Vec<BlockId> = Vec::new();

        self.lower_expr_into(fb, &e.condition)?;
        cond_blocks.push(fb.jump_if_false_later());

        let t0 = fb.new_block();
        fb.start(t0);
        self.lower_block(fb, &e.then_branch)?;
        branch_jumps.push(fb.jump_later());

        for (cond, body) in &e.elif_branches {
            let ci = fb.new_block();
            fb.start(ci);
            self.lower_expr_into(fb, cond)?;
            cond_blocks.push(fb.jump_if_false_later());
            let ti = fb.new_block();
            fb.start(ti);
            self.lower_block(fb, body)?;
            branch_jumps.push(fb.jump_later());
        }

        let eb = fb.new_block();
        fb.start(eb);
        match &e.else_branch {
            Some(b) => self.lower_block(fb, b)?,
            None => fb.emit(MirInstr::new(MirInstrKind::Unit, e.span)),
        }
        branch_jumps.push(fb.jump_later());

        let m = fb.new_block();
        fb.start(m);

        // Patch cond false-targets: c_i → next cond, last → else block.
        let mut targets: Vec<BlockId> = cond_blocks.iter().skip(1).copied().collect();
        targets.push(eb);
        for (c, t) in cond_blocks.iter().zip(targets) {
            fb.resolve(*c, t);
        }
        for j in branch_jumps {
            fb.resolve(j, m);
        }
        Ok(())
    }

    fn lower_choose(
        &mut self,
        fb: &mut FnBuilder,
        e: &TypedExprChooseHir,
    ) -> Result<(), LowerError> {
        if matches!(e.scrutinee.ty(), Type::Result(_, _)) {
            return self.lower_choose_result(fb, e);
        }
        if matches!(e.scrutinee.ty(), Type::Enum(_) | Type::App(_, _)) {
            return self.lower_choose_enum(fb, e);
        }

        // Generic choose (literals/idents, 0.0.7: or-patterns/type guards).
        self.lower_expr_into(fb, &e.scrutinee)?;
        let scr_span = e.scrutinee.span();

        let mut body_jumps: Vec<BlockId> = Vec::new();
        let mut pending: Vec<BlockId> = Vec::new();
        let mut chain_done = false;

        for arm in &e.arms {
            if chain_done {
                break;
            }
            // Expand arm pattern into alternatives (D6/D7).
            let pat = strip_type_guard(&arm.pattern);
            let alts: Vec<&TypedPatternHir> = match pat {
                TypedPatternHir::Or(parts, _) => parts.iter().map(strip_type_guard).collect(),
                other => vec![strip_type_guard(other)],
            };
            for alt_pat in &alts {
                match alt_pat {
                    TypedPatternHir::Literal(lit) => {
                        fb.emit(MirInstr::new(MirInstrKind::Dup, scr_span));
                        self.lower_expr_into(fb, lit)?;
                        fb.emit(MirInstr::new(MirInstrKind::Eq, scr_span));
                        pending.push(fb.jump_if_false_later());
                        if let Some(guard) = &arm.guard {
                            let g = fb.new_block();
                            fb.start(g);
                            self.lower_expr_into(fb, guard)?;
                            pending.push(fb.jump_if_false_later());
                        }
                        let ok = fb.new_block();
                        fb.start(ok);
                        fb.emit(MirInstr::new(MirInstrKind::Pop, scr_span));
                        self.lower_block(fb, &arm.body)?;
                        body_jumps.push(fb.jump_later());
                        // The equality and guard failures both retry at the
                        // next alternative's test point.
                        start_pending_block(fb, &mut pending);
                        // Next alternative of same or different arm starts at
                        // a new test point fed by pending failures.
                    }
                    TypedPatternHir::Ident { binding_id, .. } => {
                        let slot = self.slots.slot_for(*binding_id);
                        fb.emit(MirInstr::new(MirInstrKind::BindMatch(slot), scr_span));
                        match &arm.guard {
                            None => {
                                fb.emit(MirInstr::new(MirInstrKind::Pop, scr_span));
                                self.lower_block(fb, &arm.body)?;
                                body_jumps.push(fb.jump_later());
                                chain_done = true;
                            }
                            Some(guard) => {
                                self.lower_expr_into(fb, guard)?;
                                pending.push(fb.jump_if_false_later());
                                let ok = fb.new_block();
                                fb.start(ok);
                                fb.emit(MirInstr::new(MirInstrKind::Pop, scr_span));
                                self.lower_block(fb, &arm.body)?;
                                body_jumps.push(fb.jump_later());
                                // The guard failure retries at the next
                                // alternative's test point.
                                start_pending_block(fb, &mut pending);
                            }
                        }
                        if chain_done {
                            break;
                        }
                    }
                    TypedPatternHir::ResultCtor { span, .. } => {
                        return Err(LowerError::new(
                            LowerErrorKind::ChooseResultNeedsOkErrArms,
                            *span,
                        ));
                    }
                    TypedPatternHir::Enum { span, .. }
                    | TypedPatternHir::EnumVariant { span, .. } => {
                        return Err(LowerError::new(
                            LowerErrorKind::UnsupportedFeature {
                                feature: "enum variant pattern on a non-enum choose",
                            },
                            *span,
                        ));
                    }
                    TypedPatternHir::Or(..) | TypedPatternHir::TypeGuard { .. } => {
                        // Top-level or-patterns are flattened above and type
                        // guards stripped, so an `Or`/`TypeGuard` here is one
                        // nested directly inside a literal/ident alternative.
                        // The enum/Result payload paths lower that (the
                        // sub-pattern machinery is recursive); the generic
                        // chain does not, so it stays a defensive error.
                        return Err(LowerError::new(
                            LowerErrorKind::UnsupportedFeature {
                                feature: "nested or/type-guard in generic choose (0.0.7 T06)",
                            },
                            scr_span,
                        ));
                    }
                    TypedPatternHir::Error => {
                        return Err(LowerError::new(
                            LowerErrorKind::UnsupportedFeature {
                                feature: "error-recovery pattern in choose",
                            },
                            arm.body.span,
                        ));
                    }
                }
            }
        }

        // Every alternative resolved its own failure jumps into the next
        // test point, so nothing is pending here — a taken ident arm ends
        // the chain without failure edges.
        debug_assert!(
            pending.is_empty(),
            "each alternative resolves the failure jumps it creates"
        );
        if !chain_done {
            // All alternatives failed: pop the scrutinee, yield unit.
            fb.emit(MirInstr::new(MirInstrKind::Pop, scr_span));
            fb.emit(MirInstr::new(MirInstrKind::Unit, scr_span));
            body_jumps.push(fb.jump_later());
        }

        let m = fb.new_block();
        fb.start(m);
        for j in body_jumps {
            fb.resolve(j, m);
        }
        Ok(())
    }

    /// Lower a `choose` on a Result scrutinee (0.0.2 U06, generalized in 0.0.7 T06).
    fn lower_choose_result(
        &mut self,
        fb: &mut FnBuilder,
        e: &TypedExprChooseHir,
    ) -> Result<(), LowerError> {
        for arm in &e.arms {
            if !matches!(&arm.pattern, TypedPatternHir::ResultCtor { .. }) {
                return Err(LowerError::new(
                    LowerErrorKind::UnsupportedFeature {
                        feature: "non-`Ok`/`Err` arm in a Result `choose` (deferred to U07)",
                    },
                    choose_arm_span(arm),
                ));
            }
        }

        // Simple fast path: all arms are unguarded Ok/Err with plain Ident payloads.
        let simple = e.arms.iter().all(|arm| {
            arm.guard.is_none()
                && matches!(
                    &arm.pattern,
                    TypedPatternHir::ResultCtor { payload, .. }
                        if matches!(payload.as_ref(), TypedPatternHir::Ident { .. })
                )
        });

        self.lower_expr_into(fb, &e.scrutinee)?;
        let dk = dup_kind(&e.scrutinee.ty());
        fb.emit(MirInstr::new(dk.clone(), e.scrutinee.span()));
        fb.emit(MirInstr::new(MirInstrKind::IsErr, e.scrutinee.span()));
        let jump_err = fb.jump_if_true_later();

        if simple {
            // Preserve 0.0.6/U06 structure for compatibility.
            let mut ok_jumps: Vec<BlockId> = Vec::new();
            let mut has_ok_arm = false;
            for arm in &e.arms {
                if let TypedPatternHir::ResultCtor {
                    ctor: ResultCtor::Ok,
                    payload,
                    span,
                    ..
                } = &arm.pattern
                {
                    has_ok_arm = true;
                    if let TypedPatternHir::Ident { binding_id, .. } = payload.as_ref() {
                        let slot = self.slots.slot_for(*binding_id);
                        let arm_ok = fb.new_block();
                        fb.start(arm_ok);
                        fb.emit(MirInstr::new(MirInstrKind::UnwrapOk, *span));
                        fb.emit(MirInstr::new(MirInstrKind::StoreLocal(slot), *span));
                        self.lower_block(fb, &arm.body)?;
                        ok_jumps.push(fb.jump_later());
                    }
                }
            }

            let mut err_jumps: Vec<BlockId> = Vec::new();
            let mut first_err: Option<BlockId> = None;
            let mut has_err_arm = false;
            for arm in &e.arms {
                if let TypedPatternHir::ResultCtor {
                    ctor: ResultCtor::Err,
                    payload,
                    span,
                    ..
                } = &arm.pattern
                {
                    has_err_arm = true;
                    if let TypedPatternHir::Ident { binding_id, .. } = payload.as_ref() {
                        let slot = self.slots.slot_for(*binding_id);
                        let arm_err = fb.new_block();
                        if first_err.is_none() {
                            first_err = Some(arm_err);
                        }
                        fb.start(arm_err);
                        fb.emit(MirInstr::new(MirInstrKind::UnwrapErr, *span));
                        fb.emit(MirInstr::new(MirInstrKind::StoreLocal(slot), *span));
                        self.lower_block(fb, &arm.body)?;
                        err_jumps.push(fb.jump_later());
                    }
                }
            }

            if !has_ok_arm || !has_err_arm {
                return Err(LowerError::new(
                    LowerErrorKind::ChooseResultNeedsOkErrArms,
                    e.span,
                ));
            }

            let merge = fb.new_block();
            fb.start(merge);
            for j in ok_jumps {
                fb.resolve(j, merge);
            }
            for j in err_jumps {
                fb.resolve(j, merge);
            }
            let Some(first_err) = first_err else {
                // The `!has_err_arm` check above guarantees an Err arm was
                // visited and set `first_err`.
                unreachable!("Err arm visited, so first_err is set");
            };
            fb.resolve(jump_err, first_err);
            return Ok(());
        }

        // General path: nested payloads / guards / multiple arms per ctor.
        // Fallthrough (IsErr false) goes to ok_entry (must be physically next).
        let scr_ty = e.scrutinee.ty();
        let ok_arms: Vec<&TypedChooseArmHir> = e
            .arms
            .iter()
            .filter(|a| {
                matches!(
                    &a.pattern,
                    TypedPatternHir::ResultCtor {
                        ctor: ResultCtor::Ok,
                        ..
                    }
                )
            })
            .collect();
        if ok_arms.is_empty() {
            return Err(LowerError::new(
                LowerErrorKind::ChooseResultNeedsOkErrArms,
                e.span,
            ));
        }
        let err_arms: Vec<&TypedChooseArmHir> = e
            .arms
            .iter()
            .filter(|a| {
                matches!(
                    &a.pattern,
                    TypedPatternHir::ResultCtor {
                        ctor: ResultCtor::Err,
                        ..
                    }
                )
            })
            .collect();
        if err_arms.is_empty() {
            return Err(LowerError::new(
                LowerErrorKind::ChooseResultNeedsOkErrArms,
                e.span,
            ));
        }

        // ok_entry must be physically next after the IsErr conditional: its
        // untaken (Ok) edge falls through. The attempt chains and merge are
        // pre-created afterwards (entered only via explicit edges).
        let ok_entry = fb.new_block();

        // Pre-create the attempt chains and merge so failure edges have targets.
        // Each chain is `arms.len()` attempt blocks plus the defensive tail.
        let ok_attempts: Vec<BlockId> = (0..=ok_arms.len()).map(|_| fb.new_block()).collect();
        let err_attempts: Vec<BlockId> = (0..=err_arms.len()).map(|_| fb.new_block()).collect();
        let merge = fb.new_block();

        fb.start(ok_entry);
        let ok_inner_ty = result_payload_ty(&scr_ty, ResultCtor::Ok);
        fb.emit(MirInstr::new(MirInstrKind::UnwrapOk, e.scrutinee.span()));
        fb.jump(ok_attempts[0]);
        self.lower_result_ctor_arms(
            fb,
            &ok_arms,
            &ok_inner_ty,
            &ok_attempts,
            merge,
            e.scrutinee.span(),
        )?;

        // Err side
        let err_entry = fb.new_block();
        fb.resolve(jump_err, err_entry);
        fb.start(err_entry);
        let err_inner_ty = result_payload_ty(&scr_ty, ResultCtor::Err);
        fb.emit(MirInstr::new(MirInstrKind::UnwrapErr, e.scrutinee.span()));
        fb.jump(err_attempts[0]);
        self.lower_result_ctor_arms(
            fb,
            &err_arms,
            &err_inner_ty,
            &err_attempts,
            merge,
            e.scrutinee.span(),
        )?;

        fb.start(merge);
        Ok(())
    }

    /// Try arms for one Result constructor in order against the unwrapped
    /// payload parked on the stack. Each attempt works on a `Dup`/`DupDeep`
    /// copy so a pattern or guard failure leaves the payload parked for the
    /// next arm; the parked payload is popped only when an arm is taken.
    /// `attempts` holds one block per arm plus the defensive tail (index
    /// `arms.len()`): if every attempt fails — typeck guarantees
    /// exhaustiveness, so this is defensive — the tail pops the payload and
    /// yields unit.
    fn lower_result_ctor_arms(
        &mut self,
        fb: &mut FnBuilder,
        arms: &[&TypedChooseArmHir],
        payload_ty: &Type,
        attempts: &[BlockId],
        merge: BlockId,
        span: Span,
    ) -> Result<(), LowerError> {
        for (i, arm) in arms.iter().enumerate() {
            let on_fail = attempts[i + 1];
            fb.start(attempts[i]);
            self.lower_result_single_arm(fb, arm, payload_ty, merge, on_fail)?;
        }
        fb.start(attempts[arms.len()]);
        fb.emit(MirInstr::new(MirInstrKind::Pop, span));
        fb.emit(MirInstr::new(MirInstrKind::Unit, span));
        fb.jump(merge);
        Ok(())
    }

    /// One attempt of a Result ctor arm. Entry: the payload parked on the
    /// stack. A taken arm pops it before the body; a failed attempt leaves it
    /// parked and jumps to `on_fail` (the next attempt).
    fn lower_result_single_arm(
        &mut self,
        fb: &mut FnBuilder,
        arm: &TypedChooseArmHir,
        payload_ty: &Type,
        merge: BlockId,
        on_fail: BlockId,
    ) -> Result<(), LowerError> {
        let (payload_pat, span) = match &arm.pattern {
            TypedPatternHir::ResultCtor { payload, span, .. } => (payload.as_ref(), *span),
            _ => unreachable!("result ctor arms are pre-filtered"),
        };
        let pat = strip_type_guard(payload_pat);
        if arm.guard.is_none()
            && let TypedPatternHir::Ident { binding_id, .. } = pat
        {
            // Fast path: an unguarded ident always matches — bind the
            // parked payload (consumed) and run the body.
            let slot = self.slots.slot_for(*binding_id);
            fb.emit(MirInstr::new(MirInstrKind::StoreLocal(slot), span));
            self.lower_block(fb, &arm.body)?;
            fb.jump(merge);
            return Ok(());
        }

        // General attempt: match a copy of the payload; `lower_subpattern_match`
        // consumes the copy on both of its exits, leaving the payload parked.
        fb.emit(MirInstr::new(dup_kind(payload_ty), span));
        let pattern_fail = fb.new_block();
        let fail = FailEdge {
            on_fail: pattern_fail,
            extra_below: 0,
        };
        self.lower_subpattern_match(fb, payload_pat, payload_ty, &fail, span)?;
        if let Some(guard) = &arm.guard {
            fb.spill_for_fallthrough();
            self.lower_expr_into(fb, guard)?;
            let guard_fail = fb.jump_if_false_later();
            fb.resolve(guard_fail, on_fail);
            let guard_pass = fb.new_block();
            fb.start(guard_pass);
            fb.emit(MirInstr::new(MirInstrKind::Pop, span));
        } else {
            fb.emit(MirInstr::new(MirInstrKind::Pop, span));
        }
        self.lower_block(fb, &arm.body)?;
        fb.jump(merge);
        // Pattern mismatch: the copy is cleaned up, the payload is still parked.
        fb.start(pattern_fail);
        fb.jump(on_fail);
        Ok(())
    }

    /// Lower a `choose` on an enum scrutinee (0.0.6 T05, generalized in 0.0.7 T06).
    fn lower_choose_enum(
        &mut self,
        fb: &mut FnBuilder,
        e: &TypedExprChooseHir,
    ) -> Result<(), LowerError> {
        let scr_ty = e.scrutinee.ty();
        let scr = EnumScr {
            ty: &scr_ty,
            span: e.scrutinee.span(),
            dup: dup_kind(&scr_ty),
        };

        let mut alts: Vec<Alt<'_>> = Vec::new();
        let mut fallback: Option<&TypedChooseArmHir> = None;
        for (arm_index, arm) in e.arms.iter().enumerate() {
            let p = strip_type_guard(&arm.pattern);
            match p {
                TypedPatternHir::Enum { .. } | TypedPatternHir::EnumVariant { .. } => {
                    alts.push(Alt {
                        pat: &arm.pattern,
                        guard: arm.guard.as_deref(),
                        arm_index,
                    });
                }
                TypedPatternHir::Or(parts, _) => {
                    for part in parts {
                        alts.push(Alt {
                            pat: part,
                            guard: arm.guard.as_deref(),
                            arm_index,
                        });
                    }
                }
                _ => fallback = Some(arm),
            }
        }

        if alts.is_empty() {
            return Err(LowerError::new(
                LowerErrorKind::UnsupportedFeature {
                    feature: "enum choose with no variant arms",
                },
                e.span,
            ));
        }

        // Simple 0.0.6-compatible mode preserves the existing test block
        // layout: it applies when every non-fallback arm is a single variant
        // alternative (no or-expansion) with flat ident payloads.
        let simple_mode = {
            let variant_arm_count = e
                .arms
                .iter()
                .filter(|a| {
                    matches!(
                        strip_type_guard(&a.pattern),
                        TypedPatternHir::Enum { .. } | TypedPatternHir::EnumVariant { .. }
                    )
                })
                .count();
            let mut simple = alts.len() == variant_arm_count;
            if simple {
                for alt in &alts {
                    let p = strip_type_guard(alt.pat);
                    match p {
                        TypedPatternHir::EnumVariant { .. } => {}
                        TypedPatternHir::Enum { payloads, .. } => {
                            if !payloads.iter().all(|sp| {
                                matches!(strip_type_guard(sp), TypedPatternHir::Ident { .. })
                            }) {
                                simple = false;
                                break;
                            }
                        }
                        _ => {
                            simple = false;
                            break;
                        }
                    }
                }
            }
            simple
        };

        if simple_mode {
            return self.lower_choose_enum_simple(fb, e, scr.span, scr.dup);
        }
        self.lower_choose_enum_general(fb, e, &alts, fallback, &scr)
    }

    /// 0.0.6-compatible simple enum choose lowering (flat ident payloads, no
    /// or-patterns). Preserves the block layout expected by existing tests.
    fn lower_choose_enum_simple(
        &mut self,
        fb: &mut FnBuilder,
        e: &TypedExprChooseHir,
        scr_span: Span,
        dk: MirInstrKind,
    ) -> Result<(), LowerError> {
        let mut variant_arms: Vec<&TypedChooseArmHir> = Vec::new();
        let mut fallback: Option<&TypedChooseArmHir> = None;
        for arm in &e.arms {
            let p = strip_type_guard(&arm.pattern);
            match p {
                TypedPatternHir::Enum { .. } | TypedPatternHir::EnumVariant { .. } => {
                    variant_arms.push(arm)
                }
                _ => fallback = Some(arm),
            }
        }
        let n = variant_arms.len();
        let dup = || dk.clone();

        let test_blocks: Vec<BlockId> = (0..n).map(|_| fb.new_block()).collect();
        let fallback_blk = fb.new_block();
        let mut body_blocks: Vec<BlockId> = Vec::with_capacity(n);
        let mut guard_ok: Vec<Option<BlockId>> = Vec::with_capacity(n);
        for arm in &variant_arms {
            body_blocks.push(fb.new_block());
            guard_ok.push(if arm.guard.is_some() {
                Some(fb.new_block())
            } else {
                None
            });
        }
        let merge = fb.new_block();

        self.lower_expr_into(fb, &e.scrutinee)?;
        fb.jump(test_blocks[0]);

        for i in 0..n {
            let pat = strip_type_guard(&variant_arms[i].pattern);
            let v = match pat {
                TypedPatternHir::Enum { variant_index, .. }
                | TypedPatternHir::EnumVariant { variant_index, .. } => *variant_index,
                // `variant_arms` holds only Enum/EnumVariant patterns (the
                // pre-filter above pushed exactly those).
                _ => unreachable!("variant arm patterns are pre-filtered to Enum/EnumVariant"),
            };
            fb.start(test_blocks[i]);
            fb.emit(MirInstr::new(dup(), scr_span));
            fb.emit(MirInstr::new(MirInstrKind::EnumIs(v), scr_span));
            let j = fb.jump_if_true_later();
            fb.resolve(j, body_blocks[i]);
        }

        fb.start(fallback_blk);
        fb.emit(MirInstr::new(MirInstrKind::Pop, scr_span));
        match fallback {
            Some(arm) => self.lower_block(fb, &arm.body)?,
            None => fb.emit(MirInstr::new(MirInstrKind::Unit, scr_span)),
        }
        fb.jump(merge);

        for i in 0..n {
            let arm = variant_arms[i];
            fb.start(body_blocks[i]);
            let next_test = if i + 1 < n {
                test_blocks[i + 1]
            } else {
                fallback_blk
            };
            let pat = strip_type_guard(&arm.pattern);
            match (pat, &arm.guard) {
                (
                    TypedPatternHir::Enum {
                        payloads,
                        variant_index,
                        span,
                        ..
                    },
                    Some(guard),
                ) => {
                    self.emit_enum_payload_stores(
                        fb,
                        *variant_index,
                        payloads,
                        *span,
                        dk.clone(),
                        true,
                    );
                    self.lower_expr_into(fb, guard)?;
                    let j = fb.jump_if_false_later();
                    fb.resolve(j, next_test);
                }
                (
                    TypedPatternHir::Enum {
                        payloads,
                        variant_index,
                        span,
                        ..
                    },
                    None,
                ) => {
                    self.emit_enum_payload_stores(
                        fb,
                        *variant_index,
                        payloads,
                        *span,
                        dk.clone(),
                        false,
                    );
                    self.lower_block(fb, &arm.body)?;
                    fb.jump(merge);
                }
                (TypedPatternHir::EnumVariant { .. }, Some(guard)) => {
                    self.lower_expr_into(fb, guard)?;
                    let j = fb.jump_if_false_later();
                    fb.resolve(j, next_test);
                }
                (TypedPatternHir::EnumVariant { .. }, None) => {
                    fb.emit(MirInstr::new(MirInstrKind::Pop, scr_span));
                    self.lower_block(fb, &arm.body)?;
                    fb.jump(merge);
                }
                // `variant_arms` holds only Enum/EnumVariant patterns (the
                // pre-filter at the top of this function pushed exactly
                // those), so the (pattern, guard) matrix is exhaustive.
                _ => unreachable!("variant arm patterns are pre-filtered to Enum/EnumVariant"),
            }
        }

        for i in 0..n {
            if let Some(gb) = guard_ok[i] {
                fb.start(gb);
                fb.emit(MirInstr::new(MirInstrKind::Pop, scr_span));
                self.lower_block(fb, &variant_arms[i].body)?;
                fb.jump(merge);
            }
        }

        fb.start(merge);
        Ok(())
    }

    fn emit_enum_payload_stores(
        &mut self,
        fb: &mut FnBuilder,
        variant_index: u32,
        payloads: &[TypedPatternHir],
        span: Span,
        dk: MirInstrKind,
        staged: bool,
    ) {
        let n = payloads.len();
        for (i, p) in payloads.iter().enumerate() {
            let p = strip_type_guard(p);
            let binding_id = match p {
                TypedPatternHir::Ident { binding_id, .. } => *binding_id,
                _ => unreachable!("simple mode requires ident payloads"),
            };
            let slot = self.slots.slot_for(binding_id);
            let last = i + 1 == n;
            if staged || !last {
                fb.emit(MirInstr::new(dk.clone(), span));
            }
            fb.emit(MirInstr::new(
                MirInstrKind::EnumPayload(variant_index, i as u16),
                span,
            ));
            fb.emit(MirInstr::new(MirInstrKind::StoreLocal(slot), span));
        }
    }

    fn lower_choose_enum_general<'b>(
        &mut self,
        fb: &mut FnBuilder,
        e: &TypedExprChooseHir,
        alts: &[Alt<'b>],
        fallback: Option<&TypedChooseArmHir>,
        scr: &EnumScr,
    ) -> Result<(), LowerError> {
        use std::collections::HashMap;
        // Body blocks are shared by all alternatives of one arm (or-pattern).
        let mut arm_body_blk: HashMap<usize, BlockId> = HashMap::new();
        let mut seen_arms: Vec<usize> = Vec::new();
        for alt in alts {
            let idx = alt.arm_index;
            if !seen_arms.contains(&idx) {
                seen_arms.push(idx);
                arm_body_blk.insert(idx, fb.new_block());
            }
        }
        let fallback_blk = fb.new_block();
        let merge = fb.new_block();

        self.lower_expr_into(fb, &e.scrutinee)?;
        let alt_test_blks: Vec<BlockId> = alts.iter().map(|_| fb.new_block()).collect();
        fb.jump(alt_test_blks[0]);

        for (i, alt) in alts.iter().enumerate() {
            fb.start(alt_test_blks[i]);
            let next_fail = if i + 1 < alts.len() {
                alt_test_blks[i + 1]
            } else {
                fallback_blk
            };
            let pat = strip_type_guard(alt.pat);
            self.lower_enum_alt_match(fb, pat, alt, scr, &arm_body_blk, next_fail)?;
        }

        fb.start(fallback_blk);
        fb.emit(MirInstr::new(MirInstrKind::Pop, scr.span));
        match fallback {
            Some(arm) => self.lower_block(fb, &arm.body)?,
            None => fb.emit(MirInstr::new(MirInstrKind::Unit, scr.span)),
        }
        fb.jump(merge);

        for &idx in &seen_arms {
            let bblk = arm_body_blk[&idx];
            fb.start(bblk);
            self.lower_block(fb, &e.arms[idx].body)?;
            fb.jump(merge);
        }

        fb.start(merge);
        Ok(())
    }

    /// One flattened enum alternative: test the variant on a copy of the parked
    /// scrutinee (`Dup; EnumIs`, both edges pop the bool), extract payloads on
    /// match, evaluate the arm guard (if any) with the scrutinee still parked,
    /// and only pop the scrutinee once the arm is definitely taken. Every
    /// failure edge (variant mismatch, payload sub-pattern mismatch, guard
    /// failure) lands on `next_fail` with the parked scrutinee intact.
    fn lower_enum_alt_match<'b>(
        &mut self,
        fb: &mut FnBuilder,
        pat: &TypedPatternHir,
        alt: &Alt<'b>,
        scr: &EnumScr,
        arm_body_blk: &std::collections::HashMap<usize, BlockId>,
        next_fail: BlockId,
    ) -> Result<(), LowerError> {
        let aidx = alt.arm_index;
        let (variant_index, payloads_opt, pspan) = match pat {
            TypedPatternHir::Enum {
                variant_index,
                payloads,
                span,
                ..
            } => (*variant_index, Some(payloads.as_slice()), *span),
            TypedPatternHir::EnumVariant {
                variant_index,
                span,
                ..
            } => (*variant_index, None, *span),
            _ => unreachable!("enum alt must be Enum/EnumVariant"),
        };

        fb.spill_for_fallthrough();
        fb.emit(MirInstr::new(scr.dup.clone(), scr.span));
        fb.emit(MirInstr::new(MirInstrKind::EnumIs(variant_index), pspan));
        let fail_jmp = fb.jump_if_false_later();

        let matched_blk = fb.new_block();
        fb.start(matched_blk);

        if let Some(payloads) = payloads_opt {
            let fail = FailEdge {
                on_fail: next_fail,
                extra_below: 0,
            };
            self.lower_enum_payloads(fb, scr, pspan, variant_index, payloads, &fail)?;
        }

        // The pre-pass created one shared body block per arm with variant
        // alternatives, and every alternative carries a valid `arm_index`.
        let Some(&bblk) = arm_body_blk.get(&aidx) else {
            unreachable!("arm body block is pre-created for every alternative's arm index");
        };
        if let Some(guard) = alt.guard {
            // Guard failure retries the next alternative with the scrutinee
            // parked; guard success pops it and runs the shared arm body.
            fb.spill_for_fallthrough();
            self.lower_expr_into(fb, guard)?;
            let gf = fb.jump_if_false_later();
            fb.resolve(gf, next_fail);
            let guard_pass = fb.new_block();
            fb.start(guard_pass);
            fb.emit(MirInstr::new(MirInstrKind::Pop, scr.span));
            fb.jump(bblk);
        } else {
            fb.emit(MirInstr::new(MirInstrKind::Pop, scr.span));
            fb.jump(bblk);
        }

        fb.resolve(fail_jmp, next_fail);
        Ok(())
    }

    /// Extract and match every payload of a matched enum variant against the
    /// value on top of the stack (the parked subject copy, described by
    /// `scr`). Each extraction is `dup; EnumPayload(v, i)`, and the
    /// sub-pattern recursion runs with `fail.extra_below + 1`: the extracted
    /// payload sits above the caller's parked values, so its failure edge
    /// must clean it up on the way to `fail.on_fail` (a nested extraction
    /// passes its own count through).
    fn lower_enum_payloads(
        &mut self,
        fb: &mut FnBuilder,
        scr: &EnumScr,
        span: Span,
        variant_index: u32,
        payloads: &[TypedPatternHir],
        fail: &FailEdge,
    ) -> Result<(), LowerError> {
        for (i, subpat) in payloads.iter().enumerate() {
            fb.emit(MirInstr::new(scr.dup.clone(), span));
            let pty = enum_payload_ty(scr.ty, variant_index, i);
            fb.emit(MirInstr::new(
                MirInstrKind::EnumPayload(variant_index, i as u16),
                span,
            ));
            let inner = FailEdge {
                on_fail: fail.on_fail,
                extra_below: fail.extra_below + 1,
            };
            self.lower_subpattern_match(fb, subpat, &pty, &inner, span)?;
        }
        Ok(())
    }

    /// Match `pat` against the value on top of the stack (the subject).
    ///
    /// Contract (0.0.7 T06): on **success** the subject is consumed (bindings
    /// stored into their slots) and emission continues in the current block;
    /// on **failure** control jumps to `fail.on_fail` with the subject and its
    /// temporary copies popped, plus `fail.extra_below` re-parked values
    /// (nested extraction subjects) above the caller's base cleaned up — so
    /// the failure target always sees exactly the stack it expects.
    ///
    /// Every conditional here is preceded by [`FnBuilder::spill_for_fallthrough`]
    /// so its untaken edge lands on the continuation block created right after
    /// it, regardless of blocks the caller pre-created.
    fn lower_subpattern_match(
        &mut self,
        fb: &mut FnBuilder,
        pat: &TypedPatternHir,
        subject_ty: &Type,
        fail: &FailEdge,
        span: Span,
    ) -> Result<(), LowerError> {
        let on_fail = fail.on_fail;
        let pat = strip_type_guard(pat);
        match pat {
            TypedPatternHir::Ident { binding_id, .. } => {
                let slot = self.slots.slot_for(*binding_id);
                fb.emit(MirInstr::new(MirInstrKind::StoreLocal(slot), span));
                Ok(())
            }
            TypedPatternHir::Literal(lit) => emit_leaf_test(fb, span, fail, |fb| {
                fb.emit(MirInstr::new(dup_kind(subject_ty), span));
                self.lower_expr_into(fb, lit)?;
                fb.emit(MirInstr::new(MirInstrKind::Eq, span));
                Ok(fb.jump_if_false_later())
            }),
            TypedPatternHir::EnumVariant {
                variant_index,
                span: pspan,
                ..
            } => emit_leaf_test(fb, span, fail, |fb| {
                fb.emit(MirInstr::new(dup_kind(subject_ty), span));
                fb.emit(MirInstr::new(MirInstrKind::EnumIs(*variant_index), *pspan));
                Ok(fb.jump_if_false_later())
            }),
            TypedPatternHir::Enum {
                variant_index,
                payloads,
                span: pspan,
                ..
            } => {
                fb.spill_for_fallthrough();
                fb.emit(MirInstr::new(dup_kind(subject_ty), span));
                fb.emit(MirInstr::new(MirInstrKind::EnumIs(*variant_index), *pspan));
                let fail_jmp = fb.jump_if_false_later();
                // match falls through: extract each payload from the parked
                // subject and match it. Failure of a payload sub-pattern pops
                // the payload, the parked subject, and everything below it
                // (`extra_below + 1`), landing on `on_fail` directly.
                let mblk = fb.new_block();
                fb.start(mblk);
                let inner_scr = EnumScr {
                    ty: subject_ty,
                    span: *pspan,
                    dup: dup_kind(subject_ty),
                };
                self.lower_enum_payloads(fb, &inner_scr, *pspan, *variant_index, payloads, fail)?;
                fb.emit(MirInstr::new(MirInstrKind::Pop, *pspan));
                let cont = fb.new_block();
                fb.jump(cont);
                // fail
                let fblk = fail_cleanup_block(fb, *pspan, fail);
                fb.resolve(fail_jmp, fblk);
                fb.start(cont);
                Ok(())
            }
            TypedPatternHir::ResultCtor {
                ctor,
                payload,
                span: pspan,
                ..
            } => {
                // Test Ok/Err on a copy of the subject and unwrap it; the
                // unwrap consumes the copy, so the payload's failure edge
                // keeps the same cleanup count. `IsErr`-true jumps: to the
                // matched block for `Err`, to the failure block for `Ok` —
                // so the conditional's untaken edge (fallthrough) lands on
                // the failure block for `Err` and on the matched block for
                // `Ok`, which fixes the physical block order.
                let is_err = matches!(ctor, ResultCtor::Err);
                let inner_ty = result_payload_ty(subject_ty, *ctor);
                let cont = fb.new_block();
                fb.spill_for_fallthrough();
                fb.emit(MirInstr::new(dup_kind(subject_ty), span));
                fb.emit(MirInstr::new(MirInstrKind::IsErr, span));
                let is_err_jmp = fb.jump_if_true_later();
                let (mblk, fblk) = if is_err {
                    let fblk = fail_cleanup_block(fb, span, fail);
                    let mblk = fb.new_block();
                    fb.start(mblk);
                    fb.emit(MirInstr::new(MirInstrKind::UnwrapErr, *pspan));
                    self.lower_subpattern_match(fb, payload, &inner_ty, fail, *pspan)?;
                    (mblk, fblk)
                } else {
                    let mblk = fb.new_block();
                    fb.start(mblk);
                    fb.emit(MirInstr::new(MirInstrKind::UnwrapOk, *pspan));
                    self.lower_subpattern_match(fb, payload, &inner_ty, fail, *pspan)?;
                    let fblk = fail_cleanup_block(fb, span, fail);
                    (mblk, fblk)
                };
                fb.jump(cont);
                if is_err {
                    fb.resolve(is_err_jmp, mblk);
                } else {
                    fb.resolve(is_err_jmp, fblk);
                }
                fb.start(cont);
                Ok(())
            }
            TypedPatternHir::Or(alts, _) => {
                // Each alternative is tested against its own copy of the subject;
                // a non-last alternative's failure retries with the subject intact,
                // the last one's failure propagates to `on_fail` (subject + parked
                // values cleaned). Success of any alternative converges on `cont`
                // with the subject still on top, popped there (consumed).
                let cont = fb.new_block();
                let n = alts.len();
                for (i, alt) in alts.iter().enumerate() {
                    let last = i + 1 == n;
                    let next_alt = if last { on_fail } else { fb.new_block() };
                    fb.emit(MirInstr::new(dup_kind(subject_ty), span));
                    let alt_fail = FailEdge {
                        on_fail: next_alt,
                        extra_below: if last { fail.extra_below + 1 } else { 0 },
                    };
                    self.lower_subpattern_match(fb, alt, subject_ty, &alt_fail, span)?;
                    fb.jump(cont);
                    if !last {
                        fb.start(next_alt);
                    }
                }
                fb.start(cont);
                fb.emit(MirInstr::new(MirInstrKind::Pop, span));
                Ok(())
            }
            TypedPatternHir::Error => Err(LowerError::new(
                LowerErrorKind::UnsupportedFeature {
                    feature: "error-recovery pattern in choose",
                },
                span,
            )),
            // The function entry strips every type guard before this match
            // (D7: the ascription is compile-time only).
            TypedPatternHir::TypeGuard { .. } => unreachable!("type guards are stripped at entry"),
        }
    }
}
