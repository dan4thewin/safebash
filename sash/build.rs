fn main() {
    // Recompile when any vendored C source *or header* changes. The sources
    // live in a different package (`../sash-rt/csrc`), so cargo's default
    // watch-this-package rule does not cover them, and `cc` does not emit
    // rerun-if-changed for #included headers at all. Without this, editing
    // regcomp.c and rebuilding links a stale object and the compiler keeps
    // the old behaviour — which is exactly what happened while fixing the
    // {n,m} instruction-buffer overflow on 2026-09-13.
    for f in [
        "../sash-rt/csrc/regcomp.c",
        "../sash-rt/csrc/regaux.c",
        "../sash-rt/csrc/regcomp.h",
        "../sash-rt/csrc/regexp9.h",
        "../sash-rt/csrc/lib9.h",
        "csrc/regex_stubs.c",
    ] {
        println!("cargo:rerun-if-changed={f}");
    }
    // Link Plan 9 regex for compile-time pattern validation (DD#39)
    cc::Build::new()
        .file("../sash-rt/csrc/regcomp.c")
        .file("../sash-rt/csrc/regaux.c")
        .file("csrc/regex_stubs.c")
        .include("../sash-rt/csrc")
        .flag("-Wno-sign-compare")
        .flag("-Wno-unused-parameter")
        .compile("plan9regex_validate");
}
