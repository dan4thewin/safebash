fn main() {
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
