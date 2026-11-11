# safebash

**sash** compiles **safebash**, a deliberately constrained, Bash-derived
language, into static Linux executables. The language combines shell-style
commands and pipelines with static types, explicit input validation and
checked command failures.

safebash is not a drop-in Bash replacement. It omits shell `eval`,
implicit word splitting and other difficult-to-analyze shell behavior.
Strings, integers, typed arrays and maps have explicit rules rather than
Bash's context-dependent coercions.

The compiler is written in Rust and uses Cranelift for native code
generation. Executables link a runtime with no libc dependency;
external commands remain separate dependencies on the target system.

## A small example

Use `.sb` for safebash source files. Save this example as `total.sb`:

```bash
int total=0
while readline line {
    attest line (( amount = line ))
    (( total += amount ))
}
echo Total: $total
```

Input from `readline` starts tainted. `attest` validates each line as a
signed 64-bit integer and binds the converted value to `amount`. Invalid
input aborts before a total is printed.

These checks are not a sandbox or a proof that a validation predicate is
correct. Programs retain their operating-system permissions. Explicit trust
(`:!`) and `--no-taint` bypass taint protection; they do not validate data.
Compilation alone does not sign a binary or make it tamper-evident.

## Build and run

The current working platform is **x86-64 Linux**. AArch64 and
cross-compilation support are planned, not yet delivered. The language
and toolchain are under active development.

Building requires stable Rust, a C development toolchain, and `ld`.
From the repository root, build the separate runtime workspace, then the
compiler and Rust tools:

```bash
(cd sash-rt && cargo build --release) &&
cargo build --workspace --release
```

Alternatively, with Plan 9-style [mk](https://github.com/sgrankin/mk)
installed, use `mk -p 1 build`.

Compile and run the example:

```bash
export PATH="$PWD/target/release:$PATH"
sash total.sb -o total
printf '12\n30\n' | ./total
```

This prints `Total: 42`. Keep the build checkout: the compiler
uses its runtime archive when linking.

For existing Bash scripts, `sash-convert` assists migration:

```bash
sash-convert --annotate script.bash -o converted.sb
```

Review the generated source and its `MANUAL`/`CHECK` annotations before
compiling it with `sash converted.sb -o converted`.

Run `sash --help` for compiler options. `--manifest` lists external
command dependencies, and `--extract` retrieves embedded source.

## License

Except where otherwise noted, this project is licensed under the
[GNU GPL version 3 only](LICENSE).
The [sash-rt runtime](sash-rt/LICENSE) additionally carries the
[GCC Runtime Library Exception 3.1](COPYING.RUNTIME), allowing eligible
compiled programs to be distributed under other licenses.
Third-party components retain their own licenses.
The software comes without warranty; see the license for its terms.
