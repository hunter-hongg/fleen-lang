/// Regeneration helper for `flag_off_v7_output_is_pinned` (codegen/tests.rs):
/// compiles the pinned flag-off program through `crate::compile` and prints
/// the hex-encoded stream, formatted for pasting into the test constant.
/// Run via `cargo run -p fleen-compiler --example pindump`.
fn main() {
    let src = "func add(a: int, b: int): int = a + b\nfunc main(): int = add(1, 2)\n";
    let module = fleen_compiler::compile(src).expect("compile flag-off program");
    let bytes = fleen_compiler::codegen::to_bytes(&module);
    println!(
        "// stream length: {} bytes, version: {}",
        bytes.len(),
        module.version
    );
    let hex: Vec<String> = bytes.iter().map(|b| format!("{b:02x}")).collect();
    for chunk in hex.chunks(72) {
        println!("    \"{}\",", chunk.concat());
    }
}
