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
