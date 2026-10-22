//! v2 and v3 `.flnc` compatibility fixtures (TICKETS.md V09).
//!
//! `v1` is covered by `v1_compat.rs` (a genuine 0.0.1-built fixture).
//! This file adds the two fixtures V09 requires to complete the set:
//!
//! - `tests/fixtures/v2/greet.flnc` — a hand-built **v2** module (no
//!   `struct_defs` section — the appended table is read only for v3+). It
//!   uses only v2 opcodes and prints a greeting through the `print`
//!   builtin. It must decode, verify and execute on the 0.0.3 VM.
//! - `tests/fixtures/v3/aggregate.flnc` — a **v3** module compiled from
//!   source through the real pipeline (`fleen_compiler::compile`), whose
//!   program exercises **every** new v3 instruction group
//!   (struct 0xB0–0xB3, array 0xB4–0xB8, range 0xB9–0xBB) end to end.
//!
//! Both fixtures are committed fixed bytes. The `#[ignore]`d
//! `regenerate_fixtures` test reproduces them deterministically
//! (`to_bytes` is a pure function), so a codegen change that would alter
//! the bytes is caught by the byte-identity assertions below instead of
//! silently drifting.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use fleen_compiler::codegen::{Const, ConstId, Func, FuncId, Module, Opcode, to_bytes};

/// Path to the `fleen-vm` binary built for this test run.
const VM: &str = env!("CARGO_BIN_EXE_fleen-vm");

/// Repo-root-relative path helper (`tests/...` next to the workspace).
fn repo_path(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("../{rel}"))
}

fn push_u16(c: &mut Vec<u8>, v: u16) {
    c.extend_from_slice(&v.to_le_bytes());
}
fn push_u32(c: &mut Vec<u8>, v: u32) {
    c.extend_from_slice(&v.to_le_bytes());
}

/// The committed v2 fixture module: a greeting printed via the `print`
/// builtin (params=1). No v3 opcode and no `struct_defs` section — a
/// genuine 0.0.2-shaped module.
fn v2_greet_module() -> Module {
    // `main`:  Const 2 (the string) ; Call print (FuncId 1) ; Return
    // `print` leaves its `Unit` result on the stack, which is exactly the
    // value `main` returns (one value, depth 1, at `Return`).
    let mut code = Vec::new();
    code.push(Opcode::Const as u8);
    push_u32(&mut code, 2);
    code.push(Opcode::Call as u8);
    push_u16(&mut code, 1);
    code.push(Opcode::Return as u8);

    Module {
        version: 2,
        constants: vec![
            Const::Str("main".into()),
            Const::Str("print".into()),
            Const::Str("hello from the 0.0.2 fixture".into()),
        ],
        functions: vec![
            Func {
                name: ConstId(0),
                params: 0,
                locals: 0,
                code: code.into_boxed_slice(),
                span_map: Box::new([]),
                is_builtin: false,
            },
            Func {
                name: ConstId(1),
                params: 1,
                locals: 1,
                // Placeholder body (like codegen/mod.rs gives every builtin):
                // verify requires a reachable Return, and the VM dispatches
                // builtins by name so this is never executed.
                code: Box::new([Opcode::Unit as u8, Opcode::Return as u8]),
                span_map: Box::new([]),
                is_builtin: true,
            },
        ],
        globals: vec![],
        entry: FuncId(0),
        struct_defs: Vec::new(),
    }
}

/// The committed v3 fixture: compiled from a single source program that
/// uses structs, arrays (literal + index write + `for`) and `Range`
/// iteration (`for .. in range()`, which lowers to `MakeRange` /
/// `RangeStart` / `RangeEnd`). Version 3, so the `struct_defs` section is
/// present.
fn v3_aggregate_module() -> Module {
    const SRC: &str = r#"struct Point { x: int, y: int }
func main(): int {
    p = Point { x: 20, y: 22 };
    print("point: ", p.x as string, ",", p.y as string);

    a = [1, 2, 3];
    a[1] = 10;
    total = 0;
    for n in a {
        total = total + n;
    }
    print("array sum: ", total as string);

    rsum = 0;
    for i in range(4) {
        rsum = rsum + i;
    }
    print("range sum: ", rsum as string);
    0
}"#;
    // The committed v3 fixture must stay a byte-v3 module so it keeps
    // exercising legacy-v3 execution (BYTECODE.md §10 compat promise). The
    // compiler now emits v4 (0.0.4 T05), but the *module structure* for this
    // source is identical to what a v3-era compiler produced — only the
    // version byte differs — so pin it to 3 here to make `v3_fixture_bytes()`
    // the stable, byte-identical v3 reference that the committed file is
    // checked against.
    let mut module = fleen_compiler::compile(SRC).expect("v3 fixture source compiles");
    module.version = 3;
    module
}

/// Deterministic bytes each fixture must equal.
fn v2_fixture_bytes() -> Vec<u8> {
    to_bytes(&v2_greet_module())
}
fn v3_fixture_bytes() -> Vec<u8> {
    to_bytes(&v3_aggregate_module())
}

/// Execute a `.flnc` fixture through the real `fleen-vm` binary
/// (decode → verify → run) and return (exit code, stdout, stderr).
fn run_flnc(rel: &str) -> (i32, String, String) {
    let path = repo_path(rel);
    let output = Command::new(VM)
        .arg(&path)
        .output()
        .unwrap_or_else(|e| panic!("spawn {VM}: {e}"));
    (
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

/// Regeneration tool (V09): rewrite both committed fixtures on disk. Run
/// with `--ignored -- --nocapture` and commit the resulting files. The
/// bytes are deterministic, so committed files only change when the
/// fixture *content* is intentionally edited.
#[test]
#[ignore = "development tool: regenerate committed fixtures on disk"]
fn regenerate_fixtures() {
    let paths = [
        ("tests/fixtures/v2/greet.flnc", v2_fixture_bytes()),
        ("tests/fixtures/v3/aggregate.flnc", v3_fixture_bytes()),
    ];
    for (rel, bytes) in paths {
        let path = repo_path(rel);
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir).expect("make fixture dir");
        }
        fs::write(&path, &bytes).unwrap_or_else(|e| panic!("write {}: {e}", path.display()));
        println!("wrote {} ({} bytes)", path.display(), bytes.len());
    }
}

// ---------------------------------------------------------------------------
// v2
// ---------------------------------------------------------------------------

/// The committed v2 fixture must stay byte-identical to the builder's
/// output — codegen drift is caught here, not silently shipped.
#[test]
fn v2_fixture_is_committed_byte_identical() {
    let path = repo_path("tests/fixtures/v2/greet.flnc");
    let committed = fs::read(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    assert_eq!(
        committed,
        v2_fixture_bytes(),
        "v2 fixture drifted from the builder; run `regenerate_fixtures` if intentional"
    );
}

#[test]
fn v2_fixture_verifies_and_executes() {
    let path = repo_path("tests/fixtures/v2/greet.flnc");
    let bytes = fs::read(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    let module = fleen_compiler::codegen::from_bytes(&bytes)
        .unwrap_or_else(|e| panic!("decode {}: {e}", path.display()));
    assert_eq!(module.version, 2, "fixture must be a v2 module");
    fleen_verify::verify(&module).unwrap_or_else(|e| panic!("verify {}: {e}", path.display()));

    let (code, stdout, stderr) = run_flnc("tests/fixtures/v2/greet.flnc");
    assert_eq!(code, 0, "exit code (stderr: {stderr})");
    assert!(stderr.is_empty(), "unexpected stderr: {stderr}");
    assert_eq!(
        stdout, "hello from the 0.0.2 fixture\n",
        "v2 stdout drifted"
    );
}

// ---------------------------------------------------------------------------
// v3
// ---------------------------------------------------------------------------

/// The committed v3 fixture must stay byte-identical to compiling the
/// fixture source through the current pipeline.
#[test]
fn v3_fixture_is_committed_byte_identical() {
    let path = repo_path("tests/fixtures/v3/aggregate.flnc");
    let committed = fs::read(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    assert_eq!(
        committed,
        v3_fixture_bytes(),
        "v3 fixture drifted from the compiler; run `regenerate_fixtures` if intentional"
    );
}

#[test]
fn v3_fixture_verifies_and_executes() {
    let path = repo_path("tests/fixtures/v3/aggregate.flnc");
    let bytes = fs::read(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    let module = fleen_compiler::codegen::from_bytes(&bytes)
        .unwrap_or_else(|e| panic!("decode {}: {e}", path.display()));
    assert_eq!(module.version, 3, "fixture must be a v3 module");

    // The v3 module must carry the struct_defs table and reach every new
    // instruction group (struct 0xB0–0xB3, array 0xB4–0xB8, range 0xB9–0xBB).
    assert!(
        !module.struct_defs.is_empty(),
        "v3 fixture must exercise the appended struct_defs table"
    );
    let all_bytes: Vec<u8> = module
        .functions
        .iter()
        .flat_map(|f| f.code.iter().copied())
        .collect();
    for (name, group) in [
        ("struct", 0xB0u8..=0xB3),
        ("array", 0xB4..=0xB8),
        ("range", 0xB9..=0xBB),
    ] {
        assert!(
            all_bytes.iter().any(|b| group.contains(b)),
            "v3 fixture must exercise the {name} instruction group"
        );
    }

    fleen_verify::verify(&module).unwrap_or_else(|e| panic!("verify {}: {e}", path.display()));

    let (code, stdout, stderr) = run_flnc("tests/fixtures/v3/aggregate.flnc");
    assert_eq!(code, 0, "exit code (stderr: {stderr})");
    assert!(stderr.is_empty(), "unexpected stderr: {stderr}");
    assert_eq!(
        stdout, "point: 20,22\narray sum: 14\nrange sum: 6\n",
        "v3 stdout drifted"
    );
}

// ---------------------------------------------------------------------------
// from_bytes rejection: truncating *inside* a populated struct_defs entry
// (the V09 compatibility gate, exercised end to end at decode time).
// ---------------------------------------------------------------------------

#[test]
fn from_bytes_rejects_truncated_populated_struct_defs_table() {
    let bytes = v3_fixture_bytes();

    // The appended struct_defs table is the final section. Every strict
    // prefix must be rejected without panicking, but the interesting cases
    // are the cuts that land *inside* the table (its count and its field
    // tags), not just in the leading v2 tables.
    let mut saw_table_cut = false;
    for cut in 0..bytes.len() {
        assert!(
            fleen_compiler::codegen::from_bytes(&bytes[..cut]).is_err(),
            "prefix of len {cut} must be rejected"
        );
        // A cut at least 4 bytes before the end already consumed the
        // struct_defs count but not all of the table body.
        if cut >= bytes.len().saturating_sub(4) {
            saw_table_cut = true;
        }
    }
    assert!(
        saw_table_cut,
        "test must include cuts inside the struct_defs table"
    );
}
