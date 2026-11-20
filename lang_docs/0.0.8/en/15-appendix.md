# 15 — Appendix

## 15.1 Reserved words

```
func  const  struct  enum  if  elif  else  while  for  in  choose
when  otherwise  return  import  pub  unsafe  trusted
box  ref  deref  move  clone  as  true  false
async  await  channel
ptr  addr  deref_ptr
```

- `return` is reserved but there is no `return` statement.
- `unsafe` and `trusted` are reserved for a future version.
- `Future` is not reserved; only `Future<T>` is recognized in type position.
- Reserved words are legal as module path segments and member names
  (`import std.async;`).

## 15.2 Operator precedence

Lowest to highest:

```
or
and
== !=   < > <= >=
+ - * / %
as
?
box deref move clone  -  !       (unary prefixes)
() [i] .f  await  ?              (postfix suffixes)
```

## 15.3 Compile-time error summary

The most common compile errors, with their fixes:

| Error | Cause | Fix |
|-------|-------|-----|
| cannot assign to immutable variable | writing a `const` | declare without `const` if mutability is needed |
| owned value must be transferred explicitly | bare owned variable in a consumption position | write `move x` or `clone x` |
| use of moved value | using a variable after `move` | clone instead of move, or reorder |
| move out of box / array / struct / global | `move deref b`, `move a[i]`, `move p.f`, `move g` | use the copy/clone form |
| type annotation not allowed on assignment | `x: int = v` where `x` exists | remove the annotation |
| shadowing must preserve mutability | inner/outer mutability differs | match the outer mutability or rename |
| unhandled Result | an expression statement discards a `Result` | bind it, use `?`, or `choose` over it |
| `await` applies to a Future | `await` on a non-future | await only `async func` call results |
| ref in async function | `ref` parameter on an `async func` | use an owned or Copy parameter |
| writing a global from async | field/index/qualified write to a global in async code | send the value through a channel |
| channel cannot be cloned | `clone ch` | use `std.concurrent.split(ch)` |
| channel send requires move | owned variable sent bare | write `c.send(move x)` / `c.send(clone x)` |
| UnsupportedCast | an `as` outside the three conversions | convert manually |
| ArgTypeMismatch | non-string `print` argument | add `as string` |
| PrivateAccess | private item reached cross-module | mark the declaration `pub` |
| ModuleNotFound / CircularImport | bad import path / import cycle | fix the path / break the cycle |
| ArrayTypeMismatch / CannotInferArrayType | mixed elements / empty literal | uniform elements / add a type annotation |
| `TypeConstructorNeedsArgs` (0.0.7) | a bare parameterized enum name in type position (bare `Option`) | apply the arguments: `Option<int>` |
| `ArityMismatch` / `ArityMismatchInPattern` (0.0.7) | too many type arguments / pattern payload count mismatch | match the declared parameter/payload count |
| `RecursiveEnum` (0.0.6; 0.0.7 covers parameterized payloads) | an enum (including via type arguments) references itself | break the size cycle with `box` |
| `OrPatternBindingMismatch` (0.0.7) | or-pattern alternatives bind different names/types | bind the same names with the same types |
| `FeatureDirectiveMisplaced` (0.0.8) | a `#feature` directive appears after the first declaration | move `#feature` into the file's header region (chapter 01 §1.5) |
| `UnknownFeature` (0.0.8) | the feature name in `#feature` is not recognized | use only implemented features, e.g. `#feature curry` |
| `DuplicateFeature` (0.0.8) | the same feature is enabled twice in one file | remove the extra `#feature` line |
| `FuncFormForbiddenInCurryFile` (0.0.8) | `func` is used to declare a function in a `#feature curry` file | use a two-part declaration (chapter 07 §7.1.1) |
| `SignatureNeedsFunctionType` (0.0.8) | the signature line does not hold a function type | write an arrow function type, e.g. `add: int -> int -> int` |
| `SignatureWithoutBody` (0.0.8) | a signature line has no matching function body | add a body with the same name and arity |
| `BodyWithoutSignature` (0.0.8) | a body in a flag-on file has no signature line | add the matching signature line |
| `SignatureArityMismatch` (0.0.8) | the signature type's parameter count does not match the body's | align the signature and the body's parameter count |
| `DuplicateParam` (0.0.8) | a parameter name is repeated in the body | rename the duplicate parameter |
| `PubOnCurriedBody` (0.0.8) | `pub` is written on the body of a two-part declaration | move `pub` to the signature line |
| `ModifierOnCurriedBody` (0.0.8) | `async`/`unsafe`/`trusted` is written on the body | move the modifier to the signature line |
| `PartialApplicationDiscarded` (0.0.8) | a partial application appears only as a discarded expression statement | bind it to a name or use its return value (chapter 07 §7.4.1) |

## 15.4 Runtime error summary

| Error | Cause |
|-------|-------|
| `IndexOutOfRange` | array index outside `0..len` |
| `ChannelDeadlock` | all remaining tasks are blocked on channel operations that cannot proceed |
| arithmetic error | e.g. division by zero, `abs` of the minimum `int` |
| awaited future failure | a future that failed re-raises its error at the `await` |

Runtime errors end the program with a non-zero exit code and a message
giving the source location. There is no recovery mechanism.

## 15.5 Version notes

The current toolchain implements all features described in this reference.
For programs written against earlier releases, the reserved-word changes are
the ones to watch:

| Version | New reserved words | Note |
|---------|--------------------|------|
| 0.0.2 | `deref`, `as` | |
| 0.0.3 | `struct`, `for`, `in`, `pub` | |
| 0.0.4 | `async`, `await`, `channel` | programs using these as identifiers need renaming |
| 0.0.6 | `enum` | programs using it as an identifier need renaming |
| 0.0.7 | none (additive) | non-breaking update (D10) |
| 0.0.8 | none (`feature` is not a reserved word) | non-breaking update (D16); `#feature curry` is a header directive recognized by `#` |

Feature availability by release:

| Feature | Since |
|---------|-------|
| core expressions, `func`, `if`/`while`/`choose`, `Result` type | 0.0.1 |
| `move`/`clone`, `box`/`deref`, `ref` parameters, `?`, `Ok`/`Err`, optional semicolons, `as string` | 0.0.2 |
| `struct`, arrays, `for` + Range, modules (`import`/`pub`), the std library, `flint` | 0.0.3 |
| `async`/`await`/`Future<T>`, the executor, `channel<T>` | 0.0.4 |
| `unsafe`/`trusted`, `ptr<T>`/`addr`/`deref_ptr` | 0.0.5 |
| `enum`, `foldl`/`foldr`, `std.arr.concat`/`flatten` | 0.0.6 |
| parameterized `enum Option<T>`, full multi-payload/nested/or patterns, type guards | 0.0.7 |
| `#feature curry`, two-part declarations, curried arrow types, implicit partial application | 0.0.8 |


## 0.8. 0.0.8 notes

- **Per-file feature flags** (0.0.8): `#feature curry` is written in the file's header region and affects only that file (D6). `feature` is not a reserved word.
- **Two-part declarations** (0.0.8): in a flag-on file `func` is forbidden; use a signature line + a body (chapter 07 §7.1.1).
- **Curried arrow types** (0.0.8): `int -> int -> int` is right-associative and equivalent to `(int, int) -> int`; `() -> T` is the zero-parameter function type (chapter 04 §4.2).
- **Implicit partial application** (0.0.8): a call with fewer arguments than the callee's arity yields a function over the remaining ones; partials are Copy and their captures are deep-copied (D11). A flag-off file keeps the 0.0.7 `ArityMismatch` (D16).
- Bytecode v8 adds one `PartialApply` (0xCE) and is **content-driven**: only a module that actually encodes a partial is v8; a flag-on file without partials stays v7, and every flag-off module is byte-identical to 0.0.7 (D13/D14). Non-breaking additive (D16).
- 0.0.7: parameterized enums, recursive patterns — see §0.7. 0.0.7 notes below.

## 0.7. 0.0.7 notes

- **Parameterized enums** (0.0.7): an `enum` head may declare type parameters; every use site is fully applied; the angle and bracket spellings are equivalent (D9). `is_copy` is argument-dependent (D2).
- **Recursive patterns** (0.0.7): full multi-payload destructuring, nested patterns, or-patterns, type guards (chapter 06 §6.4). Note: within one pattern a bare name may appear at most once (`when P(None, None)` is a duplicate-declaration error).
- No new reserved words; `Result[T, E]` stays builtin (D9). Non-breaking additive (D10).
- 0.0.6: `enum` reserved; tagged unions with exhaustive `choose`.
- See chapter 16 for 0.0.5 `unsafe`/`trusted`/`ptr<T>`/`addr`/`deref_ptr`.

### Historical notes

| Version | New features |
|---------|--------------|
| 0.0.6 | `enum` (patterns bound the first payload only), `fold` → `foldl` + `foldr`, `std.arr.concat`/`flatten` |
| 0.0.5 | `unsafe`/`trusted`, `ptr<T>`/`addr`/`deref_ptr`, CLI standard, release pipeline, std distribution |
