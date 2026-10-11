# Fleen 0.0.1 Implementation Task Breakdown

> **Goal**: Fibonacci can run
> **Basis**: `SPEC.md §6` compilation stages, `DESIGN.md §15` feature scope, `BYTECODE.md` instruction set

---

## Overview

| Ticket | Block | Stage | crate | Module | Output | Dependencies |
|--------|-------|-------|-------|--------|--------|--------------|
| T01 | B1 | Lex | fleen-compiler | `lexer/` | `Vec<Token>` | — |
| T02 | B2 | Parse | fleen-compiler | `parser/` | `Ast` | T01 |
| T03 | B3 | Resolve | fleen-compiler | `resolver/` | `Hir` | T02 |
| T04 | B3.5 | BindCheck | fleen-compiler | — | — | (merged into T03 Resolver, no longer a separate ticket) |
| T05 | B4 | Typeck | fleen-compiler | `typeck/` | `TypedHir` | T03 |
| T06 | B5 | Lower | fleen-compiler | `lower/` | `Mir` | T05 |
| T07 | B6 | Codegen | fleen-compiler | `codegen/` | `Bytecode` | T06 |
| T08 | B7 | Verify | fleen-verify | `src/` | PASS/FAIL | T07 (in sync) |
| T09 | B8 | Execute | fleen-vm | `src/` | Run result | T07+T08 |
| T10 | E2E | Integration | tests/ | `e2e/fib.fln` | Output match | T09 |

---

## T01: B1 Lexer — Lexical Analysis

### Files to Create
```
fleen-compiler/src/lexer/
├── mod.rs          # pub fn lex(source: &str) -> Result<Vec<Token>, LexError>
├── token.rs        # TokenKind, Token { kind, span }, LexError, Span
└── tests.rs        # Unit tests
```

### TokenKind Definition (corresponds to `SYNTAX.ebnf`)

> Actual definition in `fleen-compiler/src/lexer/token.rs`. Key points:
> - Keywords: `Func, Const, If, Elif, Else, While, Choose, When, Otherwise, Import`
> - Type keywords: `IntType, FloatType, BoolType, StringType, UnitType, ResultType, BoxType, RefType` (types are carried by dedicated tokens, not a generic `Ident`)
> - `True`/`False` are keyword tokens (not `BoolLit`)
> - Literals carry payloads: `Ident(String), IntLit(i64), FloatLit(f64), StringLit(String)`
> - Operators: `Plus, Minus, Star, Slash, Percent, Eq, Ne, Lt, Gt, Le, Ge, Assign, Bang` (`!`), `And, Or, Not` (`not`)
> - Delimiters: `LParen, RParen, LBrace, RBrace, LBracket, RBracket, Comma, Colon, Dot, Arrow, Semi`
> - Special: `Eof, Error`
> - Keywords **not** implemented in 0.0.1 (`Return, Unsafe, Trusted, Box, Ref, Move, Clone, BoolLit`) are absent from TokenKind.
> - `Semi` is the `Semicolon` from early TICKETS drafts; mandatory in 0.0.1, ASI arrives in a later version.
```rust
// Keywords
Func, Const, If, Elif, Else, While, Choose,
When, Otherwise, Return, Import, Unsafe, Trusted,
Box, Ref, Move, Clone, True, False,

// Identifiers/literals
Ident, IntLit, FloatLit, BoolLit, StringLit,

// Operators/delimiters
Plus, Minus, Star, Slash, Percent,
Eq, Ne, Lt, Gt, Le, Ge,
Assign,
And, Or, Not,
LParen, RParen, LBrace, RBrace, LBracket, RBracket,
Comma, Colon, Dot, Arrow, // -> for func type
Semicolon, // mandatory in 0.0.1, ASI arrives in a later version

// Special
Eof, Error,
```

### Implementation Requirements
- **Preallocation**: `Vec::with_capacity(source.len() / 4)`
- **Span**: `struct Span { start: u32, end: u32 }` (byte offsets; line/column can be computed later)
- **Errors**: `LexError { kind: LexErrorKind, span }`, `kind ∈ { InvalidChar(char), UnterminatedString, UnterminatedComment, InvalidEscape(char), InvalidNumber(String), IntOverflow, FloatOverflow }`; messages produced by `Display`
- **Skip comments**: `// ...` and `/* ... */` (non-nesting)

### Tests
- `tests/lexer/valid/*.fln`: at least 1 case per token type
- `tests/lexer/invalid/*.fln`: accurate error positions, readable messages

### Acceptance
```
cargo test -p fleen-compiler lexer::tests
cargo fmt -- --check && cargo clippy -- -D warnings
```

---

## T02: B2 Parser — Syntax Analysis

### Files to Create
```
fleen-compiler/src/parser/
├── mod.rs          # pub fn parse(tokens: Vec<Token>) -> Result<Ast, ParseError>
├── ast.rs          # AST node definitions
├── expr.rs         # Expression parsing (precedence climbing)
├── stmt.rs         # Statement/declaration parsing
├── error.rs        # ParseError { span, message, expected, found }
└── tests.rs
```

### AST Nodes (correspond to `SYNTAX.ebnf`, named per `SPEC.md §3`: `ExprIf`, `StmtLet`, etc.)
```rust
// Program
pub struct Ast { pub items: Vec<Item>, pub span: Span }

pub enum Item {
    Import(ImportDecl),
    Decl(Decl),
    Expr(Expr), // Expression statement
}

// Declarations
pub enum Decl {
    Func(FuncDecl),
    VarBinding(VarBinding),
    ConstDecl(ConstDecl),
}

pub struct FuncDecl {
    pub name: Ident,
    pub params: Vec<Param>,
    pub ret_type: Option<Type>,
    pub body: FuncBody,
    pub span: Span,
}

pub enum FuncBody {
    SingleExpr(Box<Expr>),  // = expr
    Block(Block),           // { ... }
}

pub struct Param { pub name: Ident, pub ty: Type, pub span: Span }

pub struct VarBinding {
    pub name: Ident,
    pub ty: Option<Type>,
    pub init: Box<Expr>,
    pub span: Span,
}

pub struct ConstDecl {
    pub name: Ident,
    pub ty: Option<Type>,
    pub init: Box<Expr>,
    pub span: Span,
}

// Types
pub enum Type {
    Base(BaseType),           // int, float, bool, string, unit
    Array(Box<Type>),         // [T]
    Box(Box<Type>),           // box<T> (parsed in 0.0.1 but generates no instructions)
    Ref(Box<Type>),           // ref T (parsed in 0.0.1 but generates no instructions)
    Result(Box<Type>, Box<Type>), // Result<T, E>
    Func(Vec<Type>, Box<Type>),   // (T...) -> T
}

// Expressions (low to high precedence)
pub enum Expr {
    Assign(Box<Expr>, Box<Expr>),  // lhs = rhs
    If(ExprIf),
    While(ExprWhile),
    Choose(ExprChoose),
    Or(Box<Expr>, Box<Expr>),
    And(Box<Expr>, Box<Expr>),
    Eq(Box<Expr>, Box<Expr>),
    Ne(Box<Expr>, Box<Expr>),
    Lt(Box<Expr>, Box<Expr>),
    Gt(Box<Expr>, Box<Expr>),
    Le(Box<Expr>, Box<Expr>),
    Ge(Box<Expr>, Box<Expr>),
    Add(Box<Expr>, Box<Expr>),
    Sub(Box<Expr>, Box<Expr>),
    Mul(Box<Expr>, Box<Expr>),
    Div(Box<Expr>, Box<Expr>),
    Mod(Box<Expr>, Box<Expr>),
    Not(Box<Expr>),
    Neg(Box<Expr>),
    Call(Box<Expr>, Vec<Expr>),     // func(args...)
    Index(Box<Expr>, Box<Expr>),    // arr[idx]
    Field(Box<Expr>, Ident),        // obj.field
    Int(i64),
    Float(f64),
    Bool(bool),
    Str(String),
    Ident(Ident),
    Unit,
    Block(Block),
}

pub struct ExprIf {
    pub condition: Box<Expr>,
    pub then_branch: Block,
    pub elif_branches: Vec<(Box<Expr>, Block)>,
    pub else_branch: Option<Block>,
    pub span: Span,
}

pub struct ExprWhile {
    pub condition: Box<Expr>,
    pub body: Block,
    pub span: Span,
}

pub struct ExprChoose {
    pub scrutinee: Box<Expr>,
    pub arms: Vec<ChooseArm>,
    pub span: Span,
}

pub struct ChooseArm {
    pub pattern: Pattern,
    pub guard: Option<Box<Expr>>,
    pub body: Block,
}

pub enum Pattern {
    Literal(Expr), // int/float/bool/string
    Ident(Ident),  // Bound variable
}

pub struct Block { pub stmts: Vec<Stmt>, pub span: Span }

pub enum Stmt {
    Decl(Decl),
    Expr(Expr),
}
```

### Implementation Requirements
- **Recursive descent + precedence climbing** (`expr.rs`)
- **Error recovery**: keep parsing after errors, collect multiple errors (optional; MVP reports only the first)
- **Span propagation**: every node carries a `span`

### Tests
- `tests/parser/valid/*.fln`: at least 1 case per grammar rule, with AST structure assertions
- `tests/parser/invalid/*.fln`: errors carry correct Spans

### Acceptance
```
cargo test -p fleen-compiler parser::tests
cargo fmt -- --check && cargo clippy -- -D warnings
```

---

## T03: B3 Resolver — Name Resolution / Scopes

### Files to Create
```
fleen-compiler/src/resolver/
├── mod.rs          # pub fn resolve(ast: Ast) -> Result<Hir, ResolveError>; import resolution is inlined
├── scope.rs        # Scope stack, binding table
├── hir.rs          # HIR nodes (with BindingId)
└── error.rs        # Error types

fleen-compiler/tests/
└── resolver_integration.rs  # Integration tests (inline + directory tests)
```

### Core Types
```rust
// ID newtypes (per SPEC.md §5)
#[derive(Copy, Clone, PartialEq, Eq, Hash)]
pub struct BindingId(u32);

// Binding info
pub struct Binding {
    pub id: BindingId,
    pub kind: BindingKind,  // Variable | Parameter | Function | Builtin
    pub mutable: bool,      // const = false
    pub span: Span,         // Span of the declaration site
    pub hir_id: HirId,
}

// Scope stack
pub struct ScopeStack {
    scopes: Vec<Scope>,
    function_stack: Vec<usize>,  // Function boundary positions (saved/restored on nesting)
}
```

ScopeKind has four variants: `Global` / `Function` / `Loop` / `Block`. `Loop` is the scope of a `while` body, where the §4.5 assignment rule applies.

### HIR Nodes (AST → HIR: Ident becomes a resolved Ident)
```rust
pub enum ExprHir {
    Int(i64, Span),
    Float(f64, Span),
    Bool(bool, Span),
    Str(String, Span),
    Ident { name: String, binding_id: BindingId, hir_id: HirId, span: Span },  // ← key change
    // ... others same as AST but children are ExprHir
}

// Function declarations carry a BindingId
pub struct FuncDeclHir {
    pub name: String,
    pub params: Vec<ParamHir>,
    pub ret_type: Option<Type>,
    pub body: FuncBodyHir,
    pub hir_id: HirId,
    pub span: Span,
}
```

### Implementation Requirements
- **Name resolution**: turn `Ident` into `Ident { name, binding_id, hir_id, span }` (every identifier in the HIR carries the **use site** span)
- **`=` three-way disambiguation**: binding / assignment / shadowing has **exactly one decision path** (§3.6); statement position and expression position share the same target lookup
- **Loop body assignment** (§4.5): inside a `while` body (including nested blocks), `x = e` targeting an outer mutable user binding is an **assignment**, not shadowing
- **Shadowing mutability consistency** (§4.3; the old T04 B3.5 scope has been merged into B3): when shadowing a variable of the same name, inner/outer mutability must match
- **const semantic checks** (old T04 scope): rebinding a const, assigning to a const, and assignments with type annotations are all errors
- **Duplicate declaration in the same scope**: duplicate `func`, parameter, `const`, or variable → `DuplicateBinding { name, first_span }`
- **Function boundaries**: shadowing checks and loop-assignment target lookup never cross function boundaries ("functions are opaque")
- **Robust scope stack**: after error paths (tail expr failure, single-expr function body failure), scopes must be exited correctly with no leaks
- **Imports**: record `import std.io` → `ImportDecl { path: Vec<String> }`, do not load modules (0.0.1 has no module system)
- **Errors must carry real Spans**: do not use `Span::new(0, 0)` as a placeholder (SPEC §4/§11). Sole exception: placeholder nodes on error-recovery paths (`StmtHir::Error`, `PatternHir::Error`) use `HirId(0)` and `Span::new(0, 0)`, because the error is already recorded and the HIR is discarded as a whole

### Errors
- `ResolveErrorKind::UndeclaredVariable { name }`
- `ResolveErrorKind::DuplicateBinding { name, first_span }`
- `ResolveErrorKind::AssignToImmutable { name }`
- `ResolveErrorKind::TypeAnnotationOnAssignment`
- `ResolveErrorKind::ShadowingMutabilityMismatch { outer_mutable, inner_mutable }`
- `ResolveErrorKind::InvalidAssignmentTarget`

### Tests
- Scope nesting, shadowing (with mutability validation), function boundary barriers, loop body assignment, scope leaks on error paths, Span correctness

### Acceptance
```
cargo test -p fleen-compiler resolver
cargo fmt -- --check && cargo clippy --all-targets -- -D warnings
```

---

## T04: B3.5 BindCheck — Shadowing/Const Semantic Checks (merged into T03)

> **Status: merged into T03, no longer implemented separately.**
>
> Shadowing mutability consistency (DESIGN.md §4.3) and const assignment/duplicate
> declaration checks (§3.5) are done together in `resolver/mod.rs` (the unified
> `=` three-way disambiguation path), so there is no separate `binder/` crate.
> Rationale: the wording of these checks (`ShadowingMutabilityMismatch`,
> `AssignToImmutable`, `DuplicateBinding`) directly depends on the resolver's
> decision of "binding or assignment"; splitting them into a separate stage
> would traverse the same scope state twice and easily drift. Error message format:

```
error: shadowing must preserve mutability
  --> file.fln:3:5
   |
 2 | x = 42
   | ^ outer `x` is mutable
 3 |     const x = 43
   |     ^^^^^ inner `const` does not match outer mutability
```

### Acceptance
```
cargo test -p fleen-compiler resolver
```

---

## T05: B4 Typeck — Type Inference & Checking

### Files to Create
```
fleen-compiler/src/typeck/
├── mod.rs          # pub fn typeck(hir: Hir) -> Result<TypedHir, Vec<TypeckError>>
├── infer.rs        # Type inference (one-way, Hindley-Milner subset) + choose exhaustiveness check
├── error.rs        # TypeckError / TypeckErrorKind
├── typed_hir.rs    # TypedHIR (every node carries a Type)
├── unify.rs        # Type unification
└── tests.rs
```

### Type System (0.0.1)
```rust
pub enum Type {
    Int,
    Float,
    Bool,
    String,
    Unit,
    Func(Vec<Type>, Box<Type>),   // (params...) -> ret
    Result(Box<Type>, Box<Type>), // Result<T, E>
    Array(Box<Type>),           // Parsing supported, kept in type checking, codegen reports UnsupportedType
    Box(Box<Type>),             // Same as above
    Ref(Box<Type>),             // Same as above
    Unsupported(String),        // Placeholder for unsupported features
}
```

### Inference Rules
- **Literals**: type is known
- **Identifiers**: look up the bound type (Resolver already bound it)
- **Binary operators**: both sides must have the same type; result type matches
- **Comparisons**: both sides must have the same type; result is `Bool`
- **Logical**: both sides `Bool`; result `Bool`
- **Unary `-`**: `Int`/`Float` → same type
- **Unary `!`**: `Bool` → `Bool`
- **Function calls**: argument types match parameters; result is the function's return type
- **If**: condition is `Bool`; then/elif/else types must unify (or implicitly convert; 0.0.1 requires exact match)
- **While**: condition is `Bool`; body type is ignored (value is `Unit`)
- **Choose**: scrutinee type `T`; every when pattern's type is compatible with `T`; all arm body types unify
- **Result**: `Ok(v)` → `Result<T, E>` where `v: T`, `Err(e)` → `Result<T, E>` where `e: E` (0.0.1 has no `Ok`/`Err` constructor syntax; this rule is deferred to a later version)

### Choose Exhaustiveness Check (key)
- **Pattern types**:
  - Literal pattern → the literal's type (e.g. `0` → `Int`)
  - Identifier pattern → bound variable, with the scrutinee's type (binding pattern)
- **Exhaustiveness decision**:
  - Has `otherwise` → exhaustive
  - No `otherwise`: exhaustive only when the scrutinee type is `Bool` and both `true`/`false` are covered; 0.0.1 has no user-defined enums, so `Bool` is the only case
  - 0.0.1 has no `Ok`/`Err` pattern syntax yet, so a `Result<T, E>` scrutinee must also be given an `otherwise` (`Ok`/`Err` patterns and exhaustiveness recognition are planned for a later version)
  - Arms with guards do not count toward exhaustiveness (the guard may not hold)

### Errors
- Error type: `TypeckError { kind: TypeckErrorKind, span: Span }`, collected into a `Vec`
- Main kinds: `TypeMismatch { expected, found }`, `AssignTypeMismatch { expected, found }`, `ChooseNotExhaustive { scrutinee_type, missing_patterns }`, `ArgTypeMismatch { index, expected, found }`, `ArityMismatch { expected, found }`, `ConditionNotBool { found }`, `InvalidOperand { op, ty }`, `NotCallable { ty }`, `UnsupportedType { ty }`, `UnsupportedFeature { feature }`, `UndefinedVariable { name }`, `InternalError { message }` (the last two are invariant backstops: report instead of failing silently when resolver/typeck disagree)

### Acceptance
```
cargo test -p fleen-compiler typeck::tests
```

---

## T06: B5 Lower — Control-Flow Flattening → MIR

### Files to Create
```
fleen-compiler/src/lower/
├── mod.rs          # pub fn lower(typed_hir: TypedHir) -> Result<Mir, LowerError>
├── mir.rs          # MIR instructions, basic blocks, function bodies
├── cfg.rs          # Control-flow graph construction
├── slots.rs        # Local variable stack-slot allocation (no SSA)
└── tests.rs
```

### MIR Design (stack-based, shaped per `BYTECODE.md §6`)
```rust
pub struct Mir {
    pub funcs: Vec<MirFunc>,
    pub globals: Vec<MirGlobal>,
}

pub struct MirFunc {
    pub func_id: FuncId,
    pub name: String,
    pub params: u16,
    pub locals: u16,           // Total slot count (including parameter slots 0..params-1)
    pub entry: BlockId,
    pub blocks: Vec<MirBlock>,
    pub is_builtin: bool,      // Builtins like print (executed on the VM host side)
    pub ret_type: Type,
}

pub struct MirGlobal {
    pub global_id: GlobalId,
    pub name: String,
    pub mutable: bool,
    pub ty: Type,
    pub init: Vec<MirInstr>,   // Leaves one initial value on top of the stack
}

pub struct MirBlock {
    pub id: BlockId,
    pub instrs: Vec<MirInstr>,
    pub terminator: Terminator,
}

pub enum Terminator {
    Return,                    // Return
    Jump(BlockId),             // Unconditional jump
    JumpIfFalse(BlockId),      // Top-of-stack bool; jump if false
    JumpIfTrue(BlockId),       // Top-of-stack bool; jump if true
    // 0.0.1 has no Switch/Call terminator (Call is a plain instruction)
}

pub enum MirInstr {
    // Constants
    ConstInt(i64),
    ConstFloat(f64),
    ConstStr(String),
    True,
    False,
    Unit,
    // Stack operations
    Pop,
    Dup,
    // Local variables (stack slots)
    LoadLocal(u16),   // slot
    StoreLocal(u16),
    // Global variables
    LoadGlobal(u16),  // GlobalId
    StoreGlobal(u16),
    // Int arithmetic
    IAdd, ISub, IMul, IDiv, IMod,
    // Float arithmetic
    FAdd, FSub, FMul, FDiv,
    // Comparison/logic
    Eq, Ne, Lt, Gt, Le, Ge,
    Not,
    NegI, NegF,
    // Function calls
    Call(FuncId),         // Direct call
    LoadFunc(FuncId),     // Push a function value
    CallValue(u8),        // Indirect call, argc
    // choose helpers
    BindMatch(u16),       // Bind the guard variable to a slot
}
```

### Lower Rules (correspond to `BYTECODE.md §6`)
- **if/elif/else** → condition `JumpIfFalse` → next branch; branch end `Jump` to the merge block
- **while** → condition block `JumpIfFalse` exits; body end `Jump` back to the condition block
- **choose** → comparison chain: each `when` emits a comparison + `JumpIfFalse`; the guard `if` comes right after the comparison
- **Binding/assignment** → `StoreLocal(slot)`; **shadowing is handled at compile time**: a new binding gets a new slot
- **Functions** → parameters occupy `slot 0..params-1`; locals are allocated starting from `params`
- **Expression statements** → compute the value → `Pop`

### Stack-Slot Allocation (no SSA)
- Walk the function body, collect all bindings (including shadowed ones), allocate a unique slot per binding
- Same-name shadowing within one scope → new slot; the old slot is no longer used
- Parameters are fixed at `0..params-1`

### Acceptance
```
cargo test -p fleen-compiler lower::tests
# Compare the generated MIR against the example shape in BYTECODE.md §6
```

---

## T07: B6 Codegen — MIR → Bytecode

### Files to Create
```
fleen-compiler/src/codegen/
├── mod.rs          # pub fn codegen(mir: Mir) -> Bytecode
├── bytecode.rs     # Bytecode structure, instruction encoding
├── encoder.rs      # MIR instruction → byte stream, constant pool dedup, jump offset resolution
└── tests.rs
```

### Bytecode Structure (per `BYTECODE.md §2`)
```rust
pub struct Module {
    pub version: u16,           // = 1
    pub constants: Vec<Const>,  // Constant pool (deduplicated)
    pub functions: Vec<Func>,
    pub globals: Vec<Global>,
    pub entry: FuncId,          // main, or a synthesized __init__ when globals exist
}

pub enum Const {
    Int(i64),
    Float(f64),
    Str(Box<str>),
}

pub struct Func {
    pub name: ConstId,
    pub params: u16,
    pub locals: u16,
    pub code: Box<[u8]>,
    pub span_map: Box<[SpanEntry]>, // left empty in 0.0.1: Box::new([])
    pub is_builtin: bool,           // Host builtin (print); the VM executes it directly and ignores code
}

pub struct Global {
    pub name: ConstId,
    pub mutable: bool,
}
```

### Instruction Encoding (per `BYTECODE.md §4-5`)
- `opcode: u8` + operands (little-endian)
- Jump operands = **absolute byte offset of the target instruction** (relative to the start of `code`)
- Two-pass encoding: first pass emits instructions + placeholder labels; second pass resolves labels to offsets

### Implementation Notes
- **Constant pool dedup**: `HashMap<Const, ConstId>` (requires `Const` to implement `Hash/Eq`)
- **span_map**: `Box::new([])` (not generated in 0.0.1)
- **Entry point**: look up a function named `main`; error if absent
- **Global initialization**: when globals exist, synthesize `__init__` (`params=0`, `locals=0`, `is_builtin=false`), emit each `MirGlobal::init` + `StoreGlobal` in declaration order, then `Call main` + `Return` at the end, and set `entry = __init__`; with no globals, `entry = main`
- **Builtin functions**: copy `MirFunc::is_builtin` verbatim into `Func::is_builtin` (`print` etc. are executed by the VM host)

### Acceptance
```
cargo test -p fleen-compiler codegen::tests
# The output bytecode passes fleen-verify (T08)
```

---

## T08: B7 Verify — Static Bytecode Validation

### Files to Create
```
fleen-verify/src/
├── main.rs         # CLI: read .flnc → verify → exit code
├── verify.rs       # The 7 validation rules
├── stack_analysis.rs # Exact abstract stack-depth analysis
└── tests.rs
```

### The 7 Validation Rules (`BYTECODE.md §8`)
1. All `Const`/`LoadFunc`/`Call` indices are in range
2. All jump targets land on **instruction boundaries**
3. Every path in every function ends with `Return` (no falling off the bottom of a function)
4. **Operand stack depth never underflows**: exact abstract stack-depth analysis
   - Each instruction has a statically known `Δdepth`
   - Propagated from the entry (depth 0) along the CFG
   - When merging branches: **all predecessor depths must agree**; any mismatch is a failure
   - The depth before executing each instruction is ≥ the instruction's minimum required depth
5. All `StoreGlobal` targets are `mutable: true` (exception: initialization writes to globals inside the entry function `__init__`)
6. No duplicate entries in the constant pool
7. Function `locals` ≥ `params`, and all `LoadLocal/StoreLocal` slots are < `locals`

### Stack-Depth Analysis Algorithm
```rust
// Worklist algorithm
fn analyze(func: &Func) -> Result<Vec<u32>, VerifyError> {
    let mut depth_at = vec![u32::MAX; func.code.len() + 1]; // depth per byte offset
    depth_at[0] = 0;
    let mut worklist = vec![0];
    
    while let Some(pc) = worklist.pop() {
        let current_depth = depth_at[pc];
        let instr = decode_at(&func.code, pc);
        let (next_pc, delta) = instr.stack_effect();
        
        // Check the prerequisite depth
        if current_depth < instr.min_stack_depth() {
            return Err(StackUnderflow { pc });
        }
        
        let next_depth = current_depth + delta;
        if depth_at[next_pc] == u32::MAX {
            depth_at[next_pc] = next_depth;
            worklist.push(next_pc);
        } else if depth_at[next_pc] != next_depth {
            return Err(StackDepthMismatch { pc: next_pc, 
                expected: depth_at[next_pc], actual: next_depth });
        }
        
        // Propagate to jump targets too
        for target in instr.jump_targets() {
            if depth_at[target] == u32::MAX {
                depth_at[target] = next_depth;
                worklist.push(target);
            } else if depth_at[target] != next_depth {
                return Err(StackDepthMismatch { ... });
            }
        }
    }
    Ok(depth_at)
}
```

### Acceptance
```
cargo test -p fleen-verify
# PASS on all bytecode produced by T07
```

---

## T09: B8 VM — Bytecode Execution

### Files to Create
```
fleen-vm/src/
├── main.rs         # CLI: read bytecode → Verify → execute → print result
├── vm.rs           # Core interpreter loop
├── value.rs        # Value enum, operation implementations
├── frame.rs        # CallFrame, stack operations
├── error.rs        # RuntimeError
└── tests.rs
```

### Value Representation (`BYTECODE.md §3.2`)
```rust
pub enum Value {
    Int(i64),
    Float(f64),
    Bool(bool),
    Str(Rc<str>),   // 0.0.1: immutable sharing
    Unit,
    Func(FuncId),   // First-class value
}
```

### Execution Model
- **Operand stack**: `Vec<Value>`
- **Call stack**: `Vec<CallFrame>`
- **Global variable table**: `Vec<Value>` (indexed by `GlobalId`)
- **Instruction dispatch**: one big `match opcode` loop

### Calling Convention (`BYTECODE.md §3.3`)
- **Direct call**: push args → `Call FuncId`
- **Indirect call**: push `Func(FuncId)` → push args → `CallValue argc`
- **Return**: pop the return value → pop the call frame → push the return value onto the caller's stack

### Error Handling
- All `RuntimeError`s exit directly with the error message printed; **not catchable**

### Entry Point
```rust
fn main() {
    let bytecode = read_bytecode(args[1]);
    fleen_verify::verify(&bytecode).expect("verification failed");
    let result = Vm::new(bytecode).run();
    println!("{:?}", result);
}
```

### Acceptance
```
cargo test -p fleen-vm
# Run the bytecode compiled from tests/e2e/fib.fln; output matches fib.expected
```

---

## T10: E2E Integration Test — Fibonacci End to End

### Test Files
```
tests/e2e/
├── fib.fln         # Complete Fibonacci program
└── fib.expected    # Expected output (one number per line)
```

### fib.fln
```fleen
func fib(n: int): int {
    if n < 2 { n }
    elif n == 2 { 1 }
    else { fib(n - 1) + fib(n - 2) }
}

func main(): int {
    x = 0;
    const limit = 10;

    while x < limit {
        print(fib(x));
        x = x + 1;
    }

    0
}
```

### fib.expected
```
0
1
1
2
3
5
8
13
21
34
```

### Run Script
```bash
# One-stop: compile + verify + execute (recommended)
cargo fln tests/e2e/valid/fib.fln
# Equivalent: cargo run -q -p fleen-vm -- tests/e2e/valid/fib.fln

# Step by step: compile alone (-o sets the output path)
cargo run -p fleen-compiler -- tests/e2e/valid/fib.fln -o /tmp/fib.flnc

# Verify
cargo run -p fleen-verify -- /tmp/fib.flnc

# Execute the bytecode
cargo run -p fleen-vm -- /tmp/fib.flnc > /tmp/fib.out

# Compare
diff /tmp/fib.out tests/e2e/valid/fib.expected
```

> **Status** (2026-10-06): Done. E2E is driven by `fleen-vm/tests/e2e.rs` and covered
> by plain `cargo test`; cases live in `tests/e2e/valid/` (stdout comparison) and
> `tests/e2e/invalid/` (exit code `.exit` + stderr keyword `.stderr`), covering
> all four compile-stage errors plus runtime division by zero / overflow / excessive recursion depth.

### CI Integration
- `cargo test` includes the E2E tests (run alone: `cargo test -p fleen-vm --test e2e`)

---

## Dependency Graph and Parallelization

```
T01 (Lexer)
    ↓
T02 (Parser)
    ↓
T03 (Resolver) ←─────────────┐
    ↓                         │
T04 (BindCheck)              │ Parallelizable: T03/T04 run serially for one person, in parallel across people
    ↓                         │
T05 (Typeck)                 │
    ↓                         │
T06 (Lower)                  │
    ↓                         │
T07 (Codegen) ←──────────────┼── T08 (Verify) developed in sync; Verify starts as soon as Codegen produces output
    ↓                         │
T09 (VM) ────────────────────┘
    ↓
T10 (E2E)
```

---

## Quality Gates (every ticket must pass)

```bash
# Format
cargo fmt -- --check

# Lint
cargo clippy -- -D warnings

# Unit tests
cargo test -p <crate> <module>::tests

# Docs
cargo doc --no-deps
```

---

## Milestones

| Milestone | Included Tickets | Marker |
|-----------|------------------|--------|
| M1: Frontend complete | T01–T04 | `Ast` → `CheckedHir` without errors |
| M2: Type system complete | T05 | `TypedHir` with all types known |
| M3: Backend complete | T06–T08 | Bytecode passes Verify |
| M4: End-to-end running | T09–T10 | `fib.fln` produces correct output |

---

## Notes

- **Not implemented in 0.0.1**: `box<T>`/`ref` instructions, `?` sugar, `struct`, `for`, `async`, generics, FFI, `unsafe`/`trusted`
- **Parsed but generates no instructions in 0.0.1**: `box<T>`, `ref T`, `[T]` types (recognized by Parser/Resolver/Typeck; Lower/Codegen report `Unimplemented` on encountering them)
- **print function**: builtin in the VM's prelude module; `std.io.print` calls map to the host's `println!`
