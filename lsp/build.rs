fn main() {
    let source = "../tree-sitter-ink/src";
    cc::Build::new()
        .include(source)
        .file(format!("{source}/parser.c"))
        .file(format!("{source}/scanner.c"))
        .warnings(false)
        .compile("tree-sitter-ink");
    println!("cargo:rerun-if-changed={source}");
}
