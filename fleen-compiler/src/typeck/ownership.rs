//! 0.0.2 U05: flow-sensitive ownership (affine) checking.
//!
//! A second pass over the `TypedHir` produced by type inference:
//!
//! - **Fills in `Access` annotations** on `Ident` nodes — read-only owned
//!   uses become `Clone`, implicit transfers at value-producing tails
//!   become `Move` (PLAN §3.1.3 rule 3). The lowering stage (U06) picks
//!   `LoadLocal` / `MoveLocal` / `CloneLocal` from these.
//! - **Tracks move state** per owned local binding (`MoveState` below) and
//!   reports `UseAfterMove` / `MaybeMovedAfterBranch` / `AssignToMoved`.
//!
//! Rules correspond one-to-one with `DESIGN.md` §10.2 / PLAN §3.1.3:
//! - consumer positions (RHS, argument, scrutinee, `box` inner, `Ok`/`Err`
//!   payload) require an explicit `move` / `clone` — checked in `infer.rs`;
//! - producing positions (function/block/branch tails) transfer implicitly;
//! - branches join: all-Moved → Moved, partial → MaybeMoved, else Alive;
//! - `while`: a move inside the body makes the loop exit `MaybeMoved`,
//!   makes the guard's uses possibly-moved (back edge), and makes in-body
//!   uses after the move definite errors;
//! - state never crosses function boundaries (rule 8);
//! - owned globals are read as deep copies and are never moved (D5).

use crate::lexer::Span;
use crate::resolver::hir::BindingId;
use crate::typeck::error::{TypeckError, TypeckErrorKind};
use crate::typeck::typed_hir::{
    Access, Type, TypedBlockHir, TypedChooseArmHir, TypedDeclHir, TypedExprChooseHir, TypedExprHir,
    TypedExprIfHir, TypedExprWhileHir, TypedFuncBodyHir, TypedFuncDeclHir, TypedHir, TypedHirItem,
    TypedPatternHir, TypedStmtHir, TypedVarBindingHir,
};
use std::collections::{HashMap, HashSet};

/// Move state of one owned local binding on a single control-flow path.
#[derive(Debug, Clone)]
enum MoveState {
    /// The binding holds a live value on this path.
    Alive,
    /// The value was definitely transferred on this path.
    Moved { span: Span },
    /// The value was transferred on *some* paths, not others.
    MaybeMoved { spans: Vec<Span> },
}

impl MoveState {
    fn is_moved(&self) -> bool {
        matches!(self, MoveState::Moved { .. } | MoveState::MaybeMoved { .. })
    }

    /// All transfer spans recorded in this state (for diagnostics).
    fn moved_spans(&self) -> Vec<Span> {
        match self {
            MoveState::Moved { span } => vec![*span],
            MoveState::MaybeMoved { spans } => spans.clone(),
            MoveState::Alive => Vec::new(),
        }
    }
}

/// The position an identifier use appears in, deciding how a bare owned
/// local is annotated:
/// - `Producer`: value-producing tail — the value transfers implicitly;
/// - `Read`: read-only use — a bare owned local deep-copies (D3);
/// - `ReadBorrow`: read-only, no deep copy (call arguments, `deref` box
///   loads) — the plain load is kept for `lower` (U06).
#[derive(Clone, Copy, PartialEq, Eq)]
enum IdentUse {
    Producer,
    Read,
    ReadBorrow,
}

/// Checking context for one function body or one top-level item.
struct Cx<'a> {
    /// Top-level variable/const bindings: owned values here are read as
    /// deep copies and may never be moved (D5).
    globals: &'a HashSet<BindingId>,
    /// Move state of owned locals: an absent key means `Alive`.
    state: HashMap<BindingId, MoveState>,
    /// Bindings declared in the current context, so a
    /// `StmtHir::Decl(Var)` can be told apart: re-declaration = assignment
    /// (resets a live binding, errors on a moved one), first declaration =
    /// fresh binding.
    declared: HashSet<BindingId>,
    errors: &'a mut Vec<TypeckError>,
}

impl<'a> Cx<'a> {
    fn new(globals: &'a HashSet<BindingId>, errors: &'a mut Vec<TypeckError>) -> Self {
        Self {
            globals,
            state: HashMap::new(),
            declared: HashSet::new(),
            errors,
        }
    }

    /// Move state of a binding on the current path (absent = `Alive`).
    fn current(&self, id: &BindingId) -> &MoveState {
        self.state.get(id).unwrap_or(&MoveState::Alive)
    }

    /// Register a definite transfer of `id` at `span`.
    fn register_moved(&mut self, id: &BindingId, span: Span) {
        self.state.insert(*id, MoveState::Moved { span });
    }

    /// Check `id` at a use site, reporting `UseAfterMove` /
    /// `MaybeMovedAfterBranch` when the value was already transferred.
    /// Returns `true` when the value is still usable.
    fn check_use(&mut self, name: &str, id: &BindingId, use_span: Span) -> bool {
        match self.current(id) {
            MoveState::Moved { span: moved_span } => {
                self.errors.push(TypeckError::new(
                    TypeckErrorKind::UseAfterMove {
                        name: name.to_string(),
                        use_span,
                        moved_span: *moved_span,
                    },
                    use_span,
                ));
                false
            }
            MoveState::MaybeMoved { spans } => {
                self.errors.push(TypeckError::new(
                    TypeckErrorKind::MaybeMovedAfterBranch {
                        name: name.to_string(),
                        use_span,
                        moved_spans: spans.clone(),
                    },
                    use_span,
                ));
                false
            }
            MoveState::Alive => true,
        }
    }

    /// Join path-exit states into the current one: a binding is `Moved`
    /// when every path moved it, `MaybeMoved` when some (but not all) did.
    /// `may_fallthrough` adds one implicit alive path (an `if` without
    /// `else`, a `choose` without `otherwise`).
    fn merge_exits(&mut self, exits: Vec<&HashMap<BindingId, MoveState>>, may_fallthrough: bool) {
        let mut ids: HashSet<BindingId> = HashSet::new();
        for exit in &exits {
            ids.extend(exit.keys());
        }
        // Replace, not update: a moved binding that no exit path moved is
        // back to alive — the pre-join state was branch-local.
        self.state.clear();
        for id in ids {
            let moved_spans: Vec<Span> = exits
                .iter()
                .filter_map(|exit| match exit.get(&id) {
                    Some(MoveState::Moved { span }) => Some(*span),
                    _ => None,
                })
                .collect();
            let all_paths_moved =
                !may_fallthrough && !exits.is_empty() && moved_spans.len() == exits.len();
            if all_paths_moved {
                self.state.insert(
                    id,
                    MoveState::Moved {
                        span: moved_spans[0],
                    },
                );
            } else {
                let mut spans: Vec<Span> = exits
                    .iter()
                    .filter_map(|exit| exit.get(&id))
                    .filter(|s| s.is_moved())
                    .flat_map(|s| s.moved_spans())
                    .collect();
                if spans.is_empty() {
                    continue; // alive on every path: nothing to remember
                }
                spans.dedup();
                self.state.insert(id, MoveState::MaybeMoved { spans });
            }
        }
    }

    /// Check one function body with a fresh state (rule 8: state never
    /// crosses the function boundary). Nested calls save/restore the
    /// enclosing context.
    fn check_function(&mut self, f: &mut TypedFuncDeclHir, nested: bool) {
        let saved_state = if nested {
            Some(self.state.clone())
        } else {
            None
        };
        let saved_declared = if nested {
            Some(std::mem::take(&mut self.declared))
        } else {
            None
        };

        self.state.clear();
        self.declared.clear();
        for param in &f.params {
            self.declared.insert(param.binding_id);
        }
        match &mut f.body {
            TypedFuncBodyHir::SingleExpr(e) => self.walk_expr(e, true),
            TypedFuncBodyHir::Block(b) => self.walk_block(b, true),
        }

        if let (Some(state), Some(declared)) = (saved_state, saved_declared) {
            self.state = state;
            self.declared = declared;
        }
    }

    /// Walk a block: statements read-only, the tail expression is
    /// producing when `tail_producer` (an `if` branch's tail yields the
    /// branch value; a `while` body's tail is discarded, so it does not).
    fn walk_block(&mut self, b: &mut TypedBlockHir, tail_producer: bool) {
        for stmt in &mut b.stmts {
            self.walk_stmt(stmt);
        }
        if tail_producer && let Some(tail) = &mut b.tail_expr {
            self.walk_expr(tail, true);
        }
    }

    fn walk_stmt(&mut self, stmt: &mut TypedStmtHir) {
        match stmt {
            TypedStmtHir::Decl(TypedDeclHir::Var(v)) => self.walk_var_binding(v),
            TypedStmtHir::Decl(TypedDeclHir::Const(c)) => {
                self.walk_expr(&mut c.init, false);
                self.declared.insert(c.binding_id);
            }
            TypedStmtHir::Decl(TypedDeclHir::Func(f)) => self.check_function(f, true),
            TypedStmtHir::Expr(e, _) => self.walk_expr(e, false),
            TypedStmtHir::Error => {}
        }
    }

    /// Statement-position binding: a fresh binding declares `Alive`; a
    /// rebind of an existing binding resets it, or reports
    /// `AssignToMoved` when the value was transferred (distinct from
    /// "undefined" — the binding still exists, its slot is just empty).
    fn walk_var_binding(&mut self, v: &mut TypedVarBindingHir) {
        self.walk_expr(&mut v.init, false);
        if self.declared.contains(&v.binding_id)
            && matches!(
                self.current(&v.binding_id),
                MoveState::Moved { .. } | MoveState::MaybeMoved { .. }
            )
        {
            self.errors.push(TypeckError::new(
                TypeckErrorKind::AssignToMoved {
                    name: v.name.clone(),
                },
                v.span,
            ));
        }
        // Fresh binding (or error recovery): the slot is live again.
        self.state.remove(&v.binding_id);
        self.declared.insert(v.binding_id);
    }

    /// Walk an expression. `producer` marks value-producing positions where
    /// a bare owned local transfers implicitly (rule 3).
    fn walk_expr(&mut self, e: &mut TypedExprHir, producer: bool) {
        match e {
            TypedExprHir::Ident {
                name,
                binding_id,
                ty,
                access,
                span,
                ..
            } => {
                let use_kind = if producer {
                    IdentUse::Producer
                } else {
                    IdentUse::Read
                };
                self.walk_ident(name, binding_id, ty, access, *span, use_kind)
            }
            // A fresh allocation consumes its inner value.
            TypedExprHir::Box(inner, _) => self.walk_expr(inner, true),
            // Loading a box does not move it: the box binding stays in its
            // slot, so the load is a plain read even for owned boxes —
            // `deref` must not clone (deep-copy) the box itself.
            TypedExprHir::Deref(inner, _) => {
                if let TypedExprHir::Ident {
                    name,
                    binding_id,
                    ty,
                    access,
                    span,
                    ..
                } = inner.as_mut()
                {
                    // State check first (a moved box is unusable), then
                    // force the load form.
                    self.walk_ident(name, binding_id, ty, access, *span, IdentUse::ReadBorrow);
                    if ty.is_owned() || self.globals.contains(binding_id) {
                        *access = Access::Copy;
                    }
                } else {
                    // Unreachable (typeck requires an identifier operand);
                    // keep the walk total for error-recovery trees.
                    self.walk_expr(inner, false);
                }
            }
            // The pointee is replaced and consumed; the box is untouched.
            TypedExprHir::AssignDeref { rhs, .. } => self.walk_expr(rhs, true),
            TypedExprHir::Assign {
                name,
                binding_id,
                rhs,
                span,
                ..
            } => {
                self.walk_expr(rhs, false);
                if self.declared.contains(binding_id) && !self.globals.contains(binding_id) {
                    if matches!(
                        self.current(binding_id),
                        MoveState::Moved { .. } | MoveState::MaybeMoved { .. }
                    ) {
                        self.errors.push(TypeckError::new(
                            TypeckErrorKind::AssignToMoved { name: name.clone() },
                            *span,
                        ));
                    }
                    self.state.remove(binding_id);
                }
            }
            TypedExprHir::If(e) => self.walk_if(e),
            TypedExprHir::While(e) => self.walk_while(e),
            TypedExprHir::Choose(e) => self.walk_choose(e),
            // Read-only operands: Copy stays Copy, bare owned locals
            // auto-`Clone` (D3), state unchanged.
            TypedExprHir::Cast(inner, _)
            | TypedExprHir::Not(inner)
            | TypedExprHir::Neg(inner)
            | TypedExprHir::Question { operand: inner, .. }
            | TypedExprHir::ResultCtor { value: inner, .. } => {
                self.walk_expr(inner, false);
            }
            TypedExprHir::Or(l, r)
            | TypedExprHir::And(l, r)
            | TypedExprHir::Eq(l, r)
            | TypedExprHir::Ne(l, r)
            | TypedExprHir::Lt(l, r)
            | TypedExprHir::Gt(l, r)
            | TypedExprHir::Le(l, r)
            | TypedExprHir::Ge(l, r)
            | TypedExprHir::Add(l, r)
            | TypedExprHir::Sub(l, r)
            | TypedExprHir::Mul(l, r)
            | TypedExprHir::Div(l, r)
            | TypedExprHir::Mod(l, r) => {
                self.walk_expr(l, false);
                self.walk_expr(r, false);
            }
            TypedExprHir::Call(f, args, _) => {
                // The callee value is a plain read; each argument's own
                // access (set by typeck) drives the transfer rules — a
                // `move` argument registers, a `clone` one does not, a
                // bare owned read auto-clones (print args, D3).
                self.walk_expr(f, false);
                for arg in args.iter_mut() {
                    self.walk_expr(arg, false);
                }
            }
            TypedExprHir::Index(a, b) => {
                // Unreachable as expressions (typeck rejects them); keep
                // the walk total for error-recovery trees.
                self.walk_expr(a, false);
                self.walk_expr(b, false);
            }
            TypedExprHir::Field(a, _) => {
                self.walk_expr(a, false);
            }
            TypedExprHir::Int(..)
            | TypedExprHir::Float(..)
            | TypedExprHir::Bool(..)
            | TypedExprHir::Str(..) => {}
            TypedExprHir::Block(b) => self.walk_block(b, true),
        }
    }

    /// An identifier use: pick its `Access` and update move state.
    ///
    /// - Copy values and function values: plain loads, no state.
    /// - `ref` parameters: borrowed reads (`lower` emits `MakeRefLocal` at
    ///   the call site from the callee's parameter types — no `Access`
    ///   needed, but `Copy` keeps the tree well-formed).
    /// - Owned globals: reads are deep copies (D5); never moved.
    /// - Owned locals: `move` registers the transfer, `clone` / bare reads
    ///   leave the state alone (a bare read deep-copies, D3 — except in
    ///   call arguments and box loads, where the plain load is kept),
    ///   producing tails transfer implicitly.
    fn walk_ident(
        &mut self,
        name: &str,
        id: &BindingId,
        ty: &Type,
        access: &mut Access,
        span: Span,
        use_kind: IdentUse,
    ) {
        if ty.is_copy() || matches!(ty, Type::Func(..)) {
            *access = Access::Copy;
            return;
        }
        if matches!(ty, Type::Ref(_)) {
            // A borrowed value: reads produce copies (`Access::Clone`,
            // from `clone s`) or plain loads; ownership is never
            // transferred out of a borrow, so no move state applies.
            // (`Access::Move` is unreachable — typeck reports
            // `MoveOfBorrowed` first.)
            return;
        }
        if self.globals.contains(id) {
            *access = Access::Clone;
            return;
        }
        match *access {
            Access::Copy => {
                if self.check_use(name, id, span) {
                    match use_kind {
                        IdentUse::Producer => {
                            *access = Access::Move;
                            self.register_moved(id, span);
                        }
                        IdentUse::Read => *access = Access::Clone,
                        IdentUse::ReadBorrow => {}
                    }
                }
            }
            Access::Move => {
                if self.check_use(name, id, span) {
                    self.register_moved(id, span);
                }
            }
            Access::Clone => {
                self.check_use(name, id, span);
            }
        }
    }

    /// An `if` expression: each branch derives from the pre-`if` snapshot;
    /// the join is per-binding (`merge_exits`).
    fn walk_if(&mut self, e: &mut TypedExprIfHir) {
        self.walk_expr(&mut e.condition, false);

        let then_exit = self.branch_exit(|cx| cx.walk_block(&mut e.then_branch, true));

        let mut elif_exits = Vec::new();
        for (cond, block) in &mut e.elif_branches {
            self.walk_expr(cond, false);
            elif_exits.push(self.branch_exit(move |cx| cx.walk_block(block, true)));
        }

        let else_exit = e
            .else_branch
            .as_mut()
            .map(|block| self.branch_exit(move |cx| cx.walk_block(block, true)));

        let mut exits: Vec<&HashMap<BindingId, MoveState>> = vec![&then_exit];
        exits.extend(elif_exits.iter());
        if let Some(exit) = &else_exit {
            exits.push(exit);
            self.merge_exits(exits, false);
        } else {
            self.merge_exits(exits, true);
        }
    }

    /// Run `f` with a fresh view of the current state and return the
    /// resulting exit state, restoring the pre-branch state.
    fn branch_exit(&mut self, f: impl FnOnce(&mut Cx<'_>)) -> HashMap<BindingId, MoveState> {
        let saved = self.state.clone();
        f(self);
        let exit = self.state.clone();
        self.state = saved;
        exit
    }

    /// A `while` loop (conservative back-edge rules, PLAN §3.1.3):
    /// - the body runs under the entry state; in-body uses after a move
    ///   are definite `UseAfterMove` errors;
    /// - the guard is re-evaluated on the back edge, so an owned local the
    ///   guard reads is "possibly moved" when the body may move it;
    /// - at loop exit such a binding is `MaybeMoved` (the body may never
    ///   have run).
    fn walk_while(&mut self, e: &mut TypedExprWhileHir) {
        let entry = self.state.clone();
        let guard_uses = collect_owned_local_uses(&e.condition, self.globals, &entry);

        self.walk_expr(&mut e.condition, false);
        // A while body's tail is discarded (the loop is unit): it is not a
        // producing position.
        self.walk_block(&mut e.body, false);
        let body_exit = self.state.clone();

        // Back-edge check on the guard.
        for (id, name, use_span) in &guard_uses {
            if entry.get(id).is_none_or(|s| !s.is_moved())
                && body_exit.get(id).is_some_and(|s| s.is_moved())
            {
                let spans = body_exit
                    .get(id)
                    .map(|s| s.moved_spans())
                    .unwrap_or_default();
                self.errors.push(TypeckError::new(
                    TypeckErrorKind::MaybeMovedAfterBranch {
                        name: name.clone(),
                        use_span: *use_span,
                        moved_spans: spans,
                    },
                    *use_span,
                ));
            }
        }

        // Exit state: the entry state, relaxed to `MaybeMoved` for every
        // binding the body may transfer.
        self.state = entry;
        for (id, s) in &body_exit {
            if !s.is_moved() {
                continue;
            }
            match self.state.get_mut(id) {
                None => {
                    self.state.insert(
                        *id,
                        MoveState::MaybeMoved {
                            spans: s.moved_spans(),
                        },
                    );
                }
                Some(MoveState::MaybeMoved { spans: prev }) => {
                    prev.extend(s.moved_spans());
                    prev.dedup();
                }
                // Already decisive (`Moved` from before the loop): keep it.
                Some(MoveState::Moved { .. }) | Some(MoveState::Alive) => {}
            }
        }
    }

    /// A `choose` expression: the scrutinee runs first (a `move` scrutinee
    /// registers the transfer, a bare owned one was rejected by typeck),
    /// then each arm runs from the post-scrutinee snapshot with its
    /// pattern binding declared alive (fresh per arm, D7).
    fn walk_choose(&mut self, e: &mut TypedExprChooseHir) {
        self.walk_expr(&mut e.scrutinee, false);
        let after_scrutinee = self.state.clone();

        let mut arm_exits: Vec<HashMap<BindingId, MoveState>> = Vec::new();
        for arm in &mut e.arms {
            let mut arm_cx = Cx::new(self.globals, self.errors);
            arm_cx.state = after_scrutinee.clone();
            arm_cx.declared = self.declared.clone();
            if let Some(binding) = pattern_binding(&arm.pattern) {
                arm_cx.state.remove(&binding);
                arm_cx.declared.insert(binding);
            }
            if let Some(guard) = &mut arm.guard {
                arm_cx.walk_expr(guard, false);
            }
            arm_cx.walk_block(&mut arm.body, true);
            arm_exits.push(arm_cx.state);
        }

        self.state = after_scrutinee;
        let may_fallthrough = !has_otherwise(&e.arms);
        let refs: Vec<&HashMap<BindingId, MoveState>> = arm_exits.iter().collect();
        self.merge_exits(refs, may_fallthrough);
    }
}

/// The binding introduced by a pattern, if any: identifier patterns and
/// `Ok(v)` / `Err(e)` payloads both bind a fresh value per arm.
fn pattern_binding(pat: &TypedPatternHir) -> Option<BindingId> {
    match pat {
        TypedPatternHir::Ident { binding_id, .. }
        | TypedPatternHir::ResultCtor { binding_id, .. } => Some(*binding_id),
        TypedPatternHir::Literal(_) | TypedPatternHir::Error => None,
    }
}

/// A pattern acts as an `otherwise` fallback when it is the wildcard form
/// (`_` or `otherwise`, parsed as an identifier pattern — see
/// `infer::check_exhaustiveness`).
fn has_otherwise(arms: &[TypedChooseArmHir]) -> bool {
    arms.iter().any(|arm| {
        matches!(
            &arm.pattern,
            TypedPatternHir::Ident { name, .. } if name == "_" || name == "otherwise"
        )
    })
}

/// Read-only scan: owned-local identifier uses in a condition expression,
/// for the `while` guard's back-edge check (PLAN §3.1.3).
fn collect_owned_local_uses(
    e: &TypedExprHir,
    globals: &HashSet<BindingId>,
    entry: &HashMap<BindingId, MoveState>,
) -> Vec<(BindingId, String, Span)> {
    let mut out: Vec<(BindingId, String, Span)> = Vec::new();
    scan_owned_local_uses(e, &mut out, globals, entry);
    out
}

fn scan_owned_local_uses(
    e: &TypedExprHir,
    out: &mut Vec<(BindingId, String, Span)>,
    globals: &HashSet<BindingId>,
    entry: &HashMap<BindingId, MoveState>,
) {
    match e {
        TypedExprHir::Ident {
            name,
            binding_id,
            ty,
            access,
            span,
            ..
        } => {
            if ty.is_owned()
                && !globals.contains(binding_id)
                && *access == Access::Copy
                && entry.get(binding_id).is_none_or(|s| !s.is_moved())
            {
                out.push((*binding_id, name.clone(), *span));
            }
        }
        TypedExprHir::Or(l, r)
        | TypedExprHir::And(l, r)
        | TypedExprHir::Eq(l, r)
        | TypedExprHir::Ne(l, r)
        | TypedExprHir::Lt(l, r)
        | TypedExprHir::Gt(l, r)
        | TypedExprHir::Le(l, r)
        | TypedExprHir::Ge(l, r)
        | TypedExprHir::Add(l, r)
        | TypedExprHir::Sub(l, r)
        | TypedExprHir::Mul(l, r)
        | TypedExprHir::Div(l, r)
        | TypedExprHir::Mod(l, r) => {
            scan_owned_local_uses(l, out, globals, entry);
            scan_owned_local_uses(r, out, globals, entry);
        }
        TypedExprHir::Cast(inner, _)
        | TypedExprHir::Not(inner)
        | TypedExprHir::Neg(inner)
        | TypedExprHir::Question { operand: inner, .. }
        | TypedExprHir::ResultCtor { value: inner, .. }
        | TypedExprHir::Box(inner, _)
        | TypedExprHir::Deref(inner, _)
        | TypedExprHir::Assign { rhs: inner, .. }
        | TypedExprHir::AssignDeref { rhs: inner, .. } => {
            scan_owned_local_uses(inner, out, globals, entry);
        }
        TypedExprHir::Call(f, args, _) => {
            scan_owned_local_uses(f, out, globals, entry);
            for arg in args {
                scan_owned_local_uses(arg, out, globals, entry);
            }
        }
        TypedExprHir::Index(a, b) => {
            scan_owned_local_uses(a, out, globals, entry);
            scan_owned_local_uses(b, out, globals, entry);
        }
        TypedExprHir::Field(a, _) => scan_owned_local_uses(a, out, globals, entry),
        TypedExprHir::Block(b) => scan_block_owned_local_uses(b, out, globals, entry),
        TypedExprHir::If(i) => {
            scan_owned_local_uses(&i.condition, out, globals, entry);
            scan_block_owned_local_uses(&i.then_branch, out, globals, entry);
            for (cond, block) in &i.elif_branches {
                scan_owned_local_uses(cond, out, globals, entry);
                scan_block_owned_local_uses(block, out, globals, entry);
            }
            if let Some(else_block) = &i.else_branch {
                scan_block_owned_local_uses(else_block, out, globals, entry);
            }
        }
        TypedExprHir::While(w) => {
            scan_owned_local_uses(&w.condition, out, globals, entry);
            scan_block_owned_local_uses(&w.body, out, globals, entry);
        }
        TypedExprHir::Choose(c) => {
            scan_owned_local_uses(&c.scrutinee, out, globals, entry);
            for arm in &c.arms {
                if let Some(guard) = &arm.guard {
                    scan_owned_local_uses(guard, out, globals, entry);
                }
                scan_block_owned_local_uses(&arm.body, out, globals, entry);
            }
        }
        TypedExprHir::Int(..)
        | TypedExprHir::Float(..)
        | TypedExprHir::Bool(..)
        | TypedExprHir::Str(..) => {}
    }
}

fn scan_block_owned_local_uses(
    b: &TypedBlockHir,
    out: &mut Vec<(BindingId, String, Span)>,
    globals: &HashSet<BindingId>,
    entry: &HashMap<BindingId, MoveState>,
) {
    for stmt in &b.stmts {
        if let TypedStmtHir::Expr(e, _) = stmt {
            scan_owned_local_uses(e, out, globals, entry);
        }
    }
    if let Some(tail) = &b.tail_expr {
        scan_owned_local_uses(tail, out, globals, entry);
    }
}

/// Run the ownership check over a typed program, filling in every
/// `Ident`'s `Access` in place.
///
/// # Arguments
/// - `program`: the `TypedHir` from type inference.
///
/// # Returns
/// Ownership errors (empty on success). Access annotations are filled in
/// regardless.
pub fn check_ownership(program: &mut TypedHir) -> Vec<TypeckError> {
    let global_ids = collect_global_ids(program);
    let mut errors: Vec<TypeckError> = Vec::new();
    for item in program.items.iter_mut() {
        let mut cx = Cx::new(&global_ids, &mut errors);
        match item {
            TypedHirItem::Decl(TypedDeclHir::Func(f)) => cx.check_function(f, false),
            // Top-level decl: the global itself is not moveable; its
            // initializer may still contain blocks with owned locals.
            TypedHirItem::Decl(TypedDeclHir::Var(v)) => cx.walk_expr(&mut v.init, false),
            TypedHirItem::Decl(TypedDeclHir::Const(c)) => cx.walk_expr(&mut c.init, false),
            TypedHirItem::Expr(e) => cx.walk_expr(e, false),
            TypedHirItem::Import(_) => {}
        }
    }
    errors
}

/// 0.0.2 U05: collect the binding IDs of top-level variables and consts.
fn collect_global_ids(program: &TypedHir) -> HashSet<BindingId> {
    let mut ids = HashSet::new();
    for item in &program.items {
        match item {
            TypedHirItem::Decl(TypedDeclHir::Var(v)) => {
                ids.insert(v.binding_id);
            }
            TypedHirItem::Decl(TypedDeclHir::Const(c)) => {
                ids.insert(c.binding_id);
            }
            _ => {}
        }
    }
    ids
}
