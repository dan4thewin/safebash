fn main() {
    // Recompile the vendored Plan 9 engine when any C source *or header*
    // changes. The `cc` crate emits rerun-if-changed for the .c files it
    // compiles, but not for #included headers (e.g. regexp9.h, which
    // defines Reprog). Without this, editing a header silently links a
    // stale object (DD#63: the Reprog.nsub field was missed this way).
    for f in [
        "csrc/regcomp.c",
        "csrc/regexec.c",
        "csrc/regaux.c",
        "csrc/heap4.c",
        "csrc/setjmp.S",
        "csrc/regcomp.h",
        "csrc/regexp9.h",
        "csrc/lib9.h",
    ] {
        println!("cargo:rerun-if-changed={f}");
    }
    cc::Build::new()
        .file("csrc/regcomp.c")
        .file("csrc/regexec.c")
        .file("csrc/regaux.c")
        .file("csrc/heap4.c")
        .file("csrc/setjmp.S")
        .include("csrc")
        .flag("-Wno-sign-compare")
        .flag("-Wno-unused-parameter")
        .flag("-fno-stack-protector")
        .flag("-ffunction-sections")
        .flag("-fdata-sections")
        .compile("plan9regexp");
}
