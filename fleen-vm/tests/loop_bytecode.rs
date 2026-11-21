//! Bytecode gate for the loop fixtures (0.0.9 T04, PLAN §9 / D6): no version
//! bump. D6 keeps 0.0.8's content-driven rule (0.0.8 T07/D13): a module
//! lifts to v8 only when it encodes a `PartialApply`, so a loop fixture —
//! which encodes none — must stay at the default version (`MODULE_VERSION`,
//! v7). PLAN §9's "writers emit v8" row resolves to this content-driven
//! version for these fixtures; the deviation is recorded in
//! docs/0.0.9/TICKETS.md T04.
//!
//! D6's flip side is pinned here too: the T04 loop-edge layout change must
//! not lift the version — no new opcode reaches the stream (`break`/
//! `continue` lower to plain `Jump`s over existing opcode rows).

use std::fs;
use std::path::PathBuf;

use fleen_compiler::{Session, codegen::MODULE_VERSION};

/// `tests/e2e/valid/<name>.fln` next to the repo root.
fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../tests/e2e/valid")
        .join(format!("{name}.fln"))
}

/// Compile a single-file fixture in-process and return its module version.
fn compiled_version(name: &str) -> u16 {
    let source =
        fs::read_to_string(fixture_path(name)).unwrap_or_else(|e| panic!("read `{name}`: {e}"));
    let mut session = Session::new();
    let root = session.add_file(format!("{name}.fln"), source);
    session
        .compile_project(root)
        .unwrap_or_else(|e| panic!("`{name}` failed to compile: {e}"))
        .version
}

#[test]
fn loop_fixtures_stay_at_the_content_driven_version() {
    // `break` in `while`: no partial encoded — plain default version.
    assert_eq!(
        compiled_version("loop_break_while"),
        MODULE_VERSION,
        "a loop fixture encodes no `PartialApply`, so it stays at the default version"
    );
    // `continue` in `for`-over-Range: the deferred-edge layout adds no
    // opcode, so the version lifts no further either (D6: no bump).
    assert_eq!(
        compiled_version("loop_continue_for_range"),
        MODULE_VERSION,
        "the loop-edge layout change must not bump the bytecode version"
    );
}
