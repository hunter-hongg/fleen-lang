# 15 — Appendix

## 15.1 Reserved words

```
func  const  struct  if  elif  else  while  for  in  choose
when  otherwise  return  import  pub  unsafe  trusted
box  ref  deref  move  clone  as  true  false
async  await  channel
ptr  addr  deref_ptr
```

- `return` is reserved but there is no `return` statement.
- `unsafe` and `trusted` are reserved since 0.0.5 (see chapter 16).
- `ptr` is not a reserved word; only `ptr<T>` is recognized in type position. `addr` and `deref_ptr` are reserved words.
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
box deref addr deref_ptr move clone  -  !  (unary prefixes)
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

| Version | New reserved words / notes | Note |
|---------|----------------------------|------|
| 0.0.2 | `deref`, `as` | |
| 0.0.3 | `struct`, `for`, `in`, `pub` | |
| 0.0.4 | `async`, `await`, `channel` | programs using these as identifiers need renaming |
| 0.0.5 | `addr`, `deref_ptr`, `unsafe`, `trusted` (reserved); `ptr<T>` type form; pointer/unsafe layer (ch. 16) | bytecode v5 |

## 15.6 0.0.5 additions

- `unsafe`/`trusted` function modifiers; `unsafe async` supported. Unsafe ops (`addr`, `deref_ptr`, `deref_ptr` write, calls to `unsafe func`) require an unsafe/trusted body (`UnsafeOpOutsideUnsafe`/`UnsafeCallOutsideUnsafe`).
- `ptr<T>`: non-Copy, move-only (`CloneOfPointer`), non-storable (`PtrNotStorable`), never crosses await/task/channel (`PtrAcrossBoundary`).
- `addr` rejects immutable bases (`AddrOfImmutable`) and `ref` params (`AddrOfRefParam`); `deref` remains box-only.
- Toolchain: clap CLI, 6-target release pipeline, std distribution/strict version pinning (`FLINT_STD_PATH → $FLINT_HOME/std/<ver> → embedded`), bytecode v5.


The current toolchain implements all features described in this reference.
For programs written against earlier releases, the reserved-word changes are
the ones to watch:

| Version | New reserved words | Note |
|---------|--------------------|------|
| 0.0.2 | `deref`, `as` | |
| 0.0.3 | `struct`, `for`, `in`, `pub` | |
| 0.0.4 | `async`, `await`, `channel` | programs using these as identifiers need renaming |

Feature availability by release:

| Feature | Since |
|---------|-------|
| core expressions, `func`, `if`/`while`/`choose`, `Result` type | 0.0.1 |
| `move`/`clone`, `box`/`deref`, `ref` parameters, `?`, `Ok`/`Err`, optional semicolons, `as string` | 0.0.2 |
| `struct`, arrays, `for` + Range, modules (`import`/`pub`), the std library, `flint` | 0.0.3 |
| `async`/`await`/`Future<T>`, the executor, `channel<T>` | 0.0.4 |
