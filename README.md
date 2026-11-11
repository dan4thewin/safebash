# sash — Safe Bash Compiler

sash defines and compiles a safe subset of Bash into static native
binaries. The subset removes constructs that make shell scripts
difficult to analyze — eval, indirect expansion, implicit
environment mutation, word splitting — and adds a taint model that
tracks external input from entry to use.

The toolchain has two binaries:

- **sash** — the compiler. Validates, type-checks, and compiles
  sash-native source to static ELF binaries.
- **sash-convert** — the converter. Transforms existing Bash scripts
  into sash-native source (mechanical coercions, annotations, attest hints).

The compiler is also a specification. What sash accepts defines the
safe subset. What it rejects gets a diagnostic with a code,
an explanation, and a suggested fix.

```
bash source → sash-convert → sash-native source → sash → static binary
```

Or for scripts already written in sash-native:

```
sash-native source → sash → static binary
```

Output binaries are static ELF linked against sash-rt. No libc.

---

## Quick Start

```bash
# Build runtime and compiler
cd sash-rt && cargo build --release && cd ..
cd sash && cargo build --release && cd ..
cd sash-convert && cargo build --release && cd ..

# Convert a Bash script to sash-native
sash-convert script.bash -o script.sh

# Compile a sash-native script
sash script.sh -o binary
./binary

# Show dependency manifest
sash --manifest script.sh

# Cross-compile with sysroot
sash --sysroot=/mnt/target script.sh
sash --sysroot=rootfs.squashfs script.sh

# Extract source from a compiled binary
sash --extract binary

# Run tests
mk test        # compiler tests
mk testall     # all suites
```

---

## The Safety Pitch

Shell scripts run in critical paths — deployment, provisioning, system
updates — but they're difficult to analyze, unsafe by default, and
unable to participate in a chain of trust.

sash fixes this by enforcing a safe subset at compile time. A reader
can look at a sash script and know what it does. The compiler can
statically verify its structure. The resulting binary can be signed and
verified before execution.

**What safety does not mean:** sash does not sandbox which commands a
script may invoke. `rm`, `mount`, and any other command are legitimate
operations. The safety model ensures the script's *intent is visible*
and its *binary is tamper-evident*.

---

## What Gets Rejected

```bash
eval "$cmd"                  # SB4001: no eval — dynamic execution is opaque
source config.sh             # SB4001: no source — use safe KVP loader
echo `uname`                 # SC2006: no backticks — use $(uname)
[ -f "$path" ]               # SC2292: no single-bracket — use [[ ]]
export FOO=bar               # SB5001: no export — declare at point of use
echo ${!ref}                 # SB4003: no indirect expansion
x=$UNTRUSTED_VAR             # SB4020: tainted — attest or validate first
```

Each diagnostic includes a code, explanation, and suggested fix.

---

## Features

### Safe Subset
No eval, no source, no indirect expansion, no backtick substitution,
no single-bracket tests, no export keyword. These are enforced by
validator rules (SB4001, SB4020, SB5001, SB5002, and others).

### Taint Tracking
All external input — environment variables, command output, argv — is
tainted until validated with `attest`. The compiler tracks taint through
assignments and rejects use of tainted values in sensitive positions.

### Known Externals
26 well-known environment variables (HOME, USER, PATH, ...) and
25 commands (hostname, whoami, date, ...) are auto-attested.

### Type System
`int`, `array`, `map` keywords. `const` for immutable scalars.
`local` for function scope.

### Declaration Coercions
`declare -i` → `int`, `declare -a` → `array`, `declare -A` → `map`,
`readonly` → `const`, `let` → `(( ))`.

### Source Conversion (sash-convert)
`sash-convert script.bash -o script.sh` transforms Bash to sash-native:
- Mechanical coercions (`declare -i` → `int`, `readonly` → `const`, etc.)
- `MANUAL` annotations for constructs needing human review
- `CHECK` hints for taint and safety
- `--annotate` adds `# was:` comments showing original constructs

### Compiler Modes (sash)
`--janet` emits Janet source (executable, used as the property
test oracle).
`--manifest` lists all external command dependencies.
`--sysroot=<path>` resolves commands against a target filesystem.
`--trace` enables runtime command tracing.
`--extract` recovers source from compiled binaries.
Every binary embeds source in a `.sash_source` ELF section.

### Regex Support
`${var:s/re/str/flags}` — sash-native regex replace expression
(new tree-sitter grammar). `var =~ s/re/str/flags` — in-place regex
mutation statement. `${var/pat/rep}` supported via glob→regex coercion.

### Janet Backend
`--janet` emits Janet IR for oracle testing — an independent
implementation that catches compiler bugs by cross-checking output
against a separate interpreter.

### Cranelift Codegen
AOT compilation produces ~16KB static ELF binaries linked against
sash-rt, a pure Rust `#![no_std]` runtime using rustix.

---

## Test Suite

457 tests: 247 codegen, 114 end-to-end, 37 Janet oracle, 59 oracle.

```bash
mk test       # run compiler tests
mk testall    # run everything
```

---

## Repo Structure

```
sash/           compiler (validates, type-checks, compiles to native)
sash-convert/   converter (bash → sash-native source transformation)
sash-rt/        runtime library (#![no_std], rustix, no libc)
sash-expand/    expansion utilities
docs/           GUIDE.md, TIMELINE.md
notes/          iwe knowledge graph (journal, decisions, tasks, status)
audit/          adversarial review results (5 models, 3 passes)
contrib/        vendored deps (plan9port-regexp, freertos-heap4)
spec/           spec test suite (236 passing)
exp/            git submodule (safebash-exp, optional, pre-alpha experiments)
mkfile          build system (Plan 9 mk, not make)
```

---

## Pipeline Detail

```
[Bash source]
     │
     ▼
[tree-sitter-bash parse]
     │
     ▼
[Validate + taint tracking]
     │
     ▼
[Lower: CST → IR + declaration coercions]
     │
     ▼
[Type check]
     │
     ▼
[Transform: IR → SSA (versioned vars, phi nodes)]
     │
     ▼
[Constant fold]
     │
     ▼
[Cranelift codegen → ld + libsash_rt.a → static ELF]
```

---

## Planned

- **Binary signing** — sign AOT artifacts, integrate with IPE/fs-verity
- **JIT + REPL** — Cranelift JIT for interactive execution

---

## Build Requirements

- Rust (stable)
- Plan 9 `mk` (not `make`)
- System `ld` for linking

---

## License

Private — not yet licensed.
