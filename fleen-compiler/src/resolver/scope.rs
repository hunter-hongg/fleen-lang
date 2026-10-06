//! Scope management for the resolver.
//!
//! Maintains a stack of scopes for variable/function lookup and declaration.
//!
//! Scope kinds (per `DESIGN.md §4`):
//! - `Global`: top-level scope.
//! - `Function`: function body scope, independent of enclosing scopes;
//!   shadow checks never cross it ("函数不生效", DESIGN.md §4.3).
//! - `Loop`: the body block of a `while` (future: `for`) loop. Inside a loop
//!   body (including nested blocks, up to the function boundary), `x = e`
//!   *assigns* to an existing outer binding instead of shadowing it
//!   (DESIGN.md §4.5).
//! - `Block`: `{}` / `if` branch / `choose` arm scope; declarations
//!   shadow-check mutability consistency.
//!
//! The function boundary is tracked with a *stack* (`function_stack`) so that
//! nested function declarations inside a body restore the enclosing boundary
//! when resolved.

use crate::lexer::Span;
use crate::resolver::hir::{BindingId, HirId};
use std::collections::HashMap;

/// Kind of a scope.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScopeKind {
    /// Top-level scope.
    Global,
    /// Function body scope. Independent of enclosing scopes (DESIGN.md §4.2/§4.3).
    Function,
    /// Loop body scope (`while` / future `for`). Loop-assignment semantics
    /// apply within (DESIGN.md §4.5).
    Loop,
    /// Block scope (`{}`, if branch, choose arm). Shadow-checks declarations.
    Block,
}

/// A single scope containing bindings.
#[derive(Debug, Clone)]
pub struct Scope {
    pub kind: ScopeKind,
    pub bindings: HashMap<String, Binding>,
}

impl Scope {
    fn new(kind: ScopeKind) -> Self {
        Self {
            kind,
            bindings: HashMap::new(),
        }
    }

    /// Declare a binding in this scope.
    ///
    /// A builtin binding may be *overwritten* by a user binding of the same
    /// name (Python-style builtin shadowing). Any other existing binding is
    /// reported back so the caller can produce a `DuplicateBinding` error.
    pub fn declare(&mut self, name: String, binding: Binding) -> Result<(), Binding> {
        match self.bindings.get(&name) {
            Some(existing) if !existing.builtin => Err(existing.clone()),
            _ => {
                self.bindings.insert(name, binding);
                Ok(())
            }
        }
    }

    /// Look up a binding in this scope only.
    pub fn get(&self, name: &str) -> Option<&Binding> {
        self.bindings.get(name)
    }
}

/// A binding in the scope (variable, const, function, parameter, builtin).
#[derive(Debug, Clone, PartialEq)]
pub struct Binding {
    pub id: BindingId,
    pub kind: BindingKind,
    pub mutable: bool,
    /// Built-in (e.g. `print`): not an assignment target, exempt from
    /// shadow-mutability checks, and overwritable by user declarations.
    pub builtin: bool,
    pub span: Span,
    pub hir_id: HirId,
}

/// Kind of binding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BindingKind {
    Variable,
    Parameter,
    Function,
    Builtin,
    /// `const` declaration: immutable, unlike `Variable`.
    Const,
}

impl BindingKind {
    /// Whether this binding is an assignment target. Builtins are not:
    /// `print = 42` *binds* a new user variable rather than assigning.
    pub fn is_assignable(self) -> bool {
        // Builtins are not assignment targets (`print = 42` binds a new
        // user variable). `Const` is "assignable" here in the sense that it
        // is a candidate target, so a const reassignment is diagnosed as
        // `AssignToImmutable` rather than silently shadowed.
        !matches!(self, BindingKind::Builtin)
    }

    /// Whether a shadowing declaration must match this binding's mutability
    /// (DESIGN.md §4.3). Only user variables participate: parameters are
    /// exempt (DESIGN.md §4.3, "参数可被遮蔽"), function names and builtins
    /// are not shadow-checked.
    pub fn shadow_checks(self) -> bool {
        matches!(self, BindingKind::Variable | BindingKind::Const)
    }
}

/// Stack of scopes for name resolution.
#[derive(Debug, Clone)]
pub struct ScopeStack {
    scopes: Vec<Scope>,
    /// Stack of indices (into `scopes`) of enclosing function scopes.
    /// The top is the nearest enclosing function boundary; shadow and
    /// loop-assignment lookups never walk past it. Empty at global level.
    function_stack: Vec<usize>,
}

impl ScopeStack {
    pub fn new() -> Self {
        Self {
            scopes: vec![Scope::new(ScopeKind::Global)],
            function_stack: Vec::new(),
        }
    }

    /// Enter a new block scope (e.g., `{}`, if branch, choose arm).
    pub fn enter_block_scope(&mut self) {
        self.scopes.push(Scope::new(ScopeKind::Block));
    }

    /// Enter a new loop-body scope (`while` / `for` body).
    pub fn enter_loop_scope(&mut self) {
        self.scopes.push(Scope::new(ScopeKind::Loop));
    }

    /// Enter a new function scope. Pushes the new boundary onto the
    /// function stack so nested functions restore it on exit.
    pub fn enter_function_scope(&mut self) {
        self.function_stack.push(self.scopes.len());
        self.scopes.push(Scope::new(ScopeKind::Function));
    }

    /// Exit the current scope. Kept in sync with `function_stack` so a
    /// nested function's scope exit restores the *enclosing* function
    /// boundary rather than clearing it.
    pub fn exit_scope(&mut self) {
        debug_assert!(self.scopes.len() > 1, "cannot exit the global scope");
        if self.scopes.len() <= 1 {
            return;
        }
        if self
            .scopes
            .last()
            .is_some_and(|s| s.kind == ScopeKind::Function)
        {
            self.function_stack.pop();
        }
        self.scopes.pop();
    }

    /// Declare a binding in the current scope. See [`Scope::declare`].
    pub fn declare(&mut self, name: String, binding: Binding) -> Result<(), Binding> {
        self.scopes
            .last_mut()
            .expect("global scope always exists")
            .declare(name, binding)
    }

    /// Get a binding from the current scope only.
    pub fn current_get(&self, name: &str) -> Option<&Binding> {
        self.scopes
            .last()
            .expect("global scope always exists")
            .get(name)
    }

    /// Look up a binding starting from the current scope, walking up parents.
    /// This crosses function boundaries — used for identifier resolution.
    pub fn get(&self, name: &str) -> Option<&Binding> {
        self.scopes.iter().rev().find_map(|s| s.get(name))
    }

    /// Get a mutable reference to a binding by its ID (searches the chain).
    /// ID-based to avoid touching a same-named binding in another scope.
    pub fn get_mut_by_id(&mut self, id: BindingId) -> Option<&mut Binding> {
        self.scopes
            .iter_mut()
            .rev()
            .find_map(|s| s.bindings.values_mut().find(|b| b.id == id))
    }

    /// The index of the nearest enclosing function scope, or 0 at global
    /// level (i.e. walk everything, including the global scope).
    fn function_boundary(&self) -> usize {
        self.function_stack.last().copied().unwrap_or(0)
    }

    /// Iterate bindings from current scope up to and including the nearest
    /// enclosing function scope, but never past it (DESIGN.md §4.3).
    pub fn iter_until_function(&self) -> impl Iterator<Item = &Scope> {
        let end = self.function_boundary();
        self.scopes[end..].iter().rev()
    }

    /// Whether the current position is inside a loop body (including nested
    /// blocks), up to the function boundary. The function boundary blocks
    /// loop context just like it blocks shadowing.
    pub fn inside_loop(&self) -> bool {
        let end = self.function_boundary();
        self.scopes[end..]
            .iter()
            .rev()
            .any(|s| s.kind == ScopeKind::Loop)
    }

    /// Find the nearest *shadow-domain* binding (`Variable` or `Parameter`)
    /// up to the function boundary. Used for shadow mutability checks and
    /// for loop-body assignment targets.
    pub fn find_shadow_domain(&self, name: &str) -> Option<&Binding> {
        self.iter_until_function().find_map(|s| {
            s.get(name).filter(|b| {
                matches!(
                    b.kind,
                    BindingKind::Variable | BindingKind::Parameter | BindingKind::Const
                )
            })
        })
    }

    /// Resolve an assignment target for `x = e` under the unified `=` rule
    /// (DESIGN.md §3.4/§4.5):
    /// 1. A non-builtin binding in the *current* scope (any kind) → assign there.
    /// 2. Otherwise, if inside a loop body, the nearest shadow-domain binding
    ///    up to the function boundary → assign there (loop special case).
    /// 3. Otherwise `None` — no assignment target exists at this position.
    ///
    /// Note: NOT a plain name lookup — identifier *uses* (reads) still walk
    /// the full chain via [`ScopeStack::get`].
    pub fn find_assign_target(&self, name: &str) -> Option<&Binding> {
        if let Some(b) = self.current_get(name)
            && b.kind.is_assignable()
        {
            return Some(b);
        }
        if self.inside_loop() {
            return self.find_shadow_domain(name);
        }
        None
    }
}

impl Default for ScopeStack {
    fn default() -> Self {
        Self::new()
    }
}
