//! 0.0.3 V06 std library source embedding.
//!
//! Each file is returned as `(path, source)`. The `path` keys the source map;
//! users see it as `std/...`. The compiler treats these as normal Fleen files
//! during project compilation. `std.collection.iter` was added in 0.0.3 V07.

pub fn files() -> Vec<(&'static str, &'static str)> {
    vec![
        ("std/mod.fln", include_str!("../../std/mod.fln")),
        ("std/io.fln", include_str!("../../std/io.fln")),
        ("std/str.fln", include_str!("../../std/str.fln")),
        ("std/num.fln", include_str!("../../std/num.fln")),
        ("std/arr.fln", include_str!("../../std/arr.fln")),
        (
            "std/collection/mod.fln",
            include_str!("../../std/collection/mod.fln"),
        ),
        (
            "std/collection/range.fln",
            include_str!("../../std/collection/range.fln"),
        ),
        (
            "std/collection/iter.fln",
            include_str!("../../std/collection/iter.fln"),
        ),
        ("std/async.fln", include_str!("../../std/async.fln")),
        (
            "std/concurrent.fln",
            include_str!("../../std/concurrent.fln"),
        ),
        ("std/os.fln", include_str!("../../std/os.fln")),
        ("std/time.fln", include_str!("../../std/time.fln")),
    ]
}

/// Load std files from an external directory.
///
/// Returns a vector of (path, source) pairs, where path is the std-relative
/// path (e.g., "std/mod.fln"). The directory structure is expected to match
/// the embedded std structure.
pub fn files_from_dir(dir: &std::path::Path) -> Result<Vec<(String, String)>, std::io::Error> {
    let mut out = Vec::new();
    collect_std_files(dir, dir, &mut out)?;
    Ok(out)
}

/// Recursively collect .fln files from a directory, preserving paths relative
/// to `root` (the std tree root). `dir` is the directory currently being
/// scanned; `root` stays fixed at the std tree root so every file gets a
/// stable `std/<relative>` name, avoiding collisions between e.g.
/// `std/mod.fln` and `std/collection/mod.fln`.
fn collect_std_files(
    root: &std::path::Path,
    dir: &std::path::Path,
    out: &mut Vec<(String, String)>,
) -> std::io::Result<()> {
    let mut entries: Vec<std::path::PathBuf> = std::fs::read_dir(dir)?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .collect();
    entries.sort();

    for entry in entries {
        if entry.is_dir() {
            collect_std_files(root, &entry, out)?;
        } else if entry.extension().and_then(|e| e.to_str()) == Some("fln") {
            let rel = entry.strip_prefix(root).map_err(|_| {
                std::io::Error::other(format!("failed to strip prefix: {}", entry.display()))
            })?;
            let rel_name = format!("std/{}", rel.to_string_lossy().replace('\\', "/"));
            let source = std::fs::read_to_string(&entry)?;
            out.push((rel_name, source));
        }
    }
    Ok(())
}
