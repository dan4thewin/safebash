fn main() {
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
