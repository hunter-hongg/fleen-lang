//! `Flint.lock` read/write (0.0.3 V08, D9: the lock is the pin).
//!
//! The lock is a committed file next to `flint.toml`. "Committed" is what
//! makes builds reproducible; a manifest change that dirties it triggers
//! re-resolution on the next build.

use std::collections::BTreeMap;
use std::path::Path;

use crate::manifest;
use crate::resolve::{ResolvedCrate, render_lock};

/// Read `Flint.lock`, returning an empty map if it does not exist yet.
pub fn read_lock(project: &Path) -> Result<BTreeMap<String, manifest::LockEntry>, String> {
    let path = project.join("Flint.lock");
    let text = match std::fs::read_to_string(&path) {
        Ok(t) => t,
        Err(_) => return Ok(BTreeMap::new()),
    };
    manifest::parse_lock(&text)
        .map(|(_, entries)| entries)
        .map_err(|e| format!("invalid {}: {e}", path.display()))
}

/// Write `Flint.lock` with the resolved crates pinned by commit.
pub fn write_lock(project: &Path, project_name: &str, crates: &[ResolvedCrate]) {
    let path = project.join("Flint.lock");
    let body = render_lock(project_name, crates);
    let _ = std::fs::write(&path, body);
}
