# sash — Safe Bash Compiler

sash compiles a safe subset of Bash into static native binaries.
Scripts that pass the compiler are predictable, analyzable, and
tamper-evident. Scripts that don't pass get clear diagnostics
explaining what to fix and why.

```
bash source → tree-sitter parse → validate + taint → lower → type check
    → SSA → constant fold → Cranelift codegen → ld → static binary (~16KB)
```

No libc. No C code. No inline assembly. Output is byte-identical to Bash.

---

## Quick Start

```bash
# Build runtime and compiler
cd sash-rt && cargo build --release && cd ..
cd sash && cargo build --release && cd ..

# Compile a script
sash script.sh -o binary
./binary

# Show coerced source
sash --emit-source script.sh
sash --emit-source --annotate script.sh   # adds "# was:" comments

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
x=$UNTRUSTED_VAR             # SB4020: tainted — bless or validate first
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
tainted until validated with `bless`. The compiler tracks taint through
assignments and rejects use of tainted values in sensitive positions.

### Known Externals
26 well-known environment variables (HOME, USER, PATH, ...) and
25 commands (hostname, whoami, date, ...) are auto-blessed.

### Type System
`int`, `array`, `map` keywords. `const` for immutable scalars.
`local` for function scope.

### Declaration Coercions
`declare -i` → `int`, `declare -a` → `array`, `declare -A` → `map`,
`readonly` → `const`, `let` → `(( ))`.

### Source Emitter
`--emit-source` shows the coerced sash-native source.
`--annotate` adds `# was:` comments showing original constructs.
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
sash/          compiler
sash-rt/       runtime library (#![no_std], rustix, no libc)
doc/           JOURNAL.md, GUIDE.md, SUBSET.md, TODO.md, TIMELINE.md
audit/         adversarial review results (5 models, 3 passes)
contrib/       vendored deps (plan9port-regexp, freertos-heap4)
suites/        brush/oils test runners and results
exp/           git submodule (safebash-exp, optional, pre-alpha experiments)
mkfile         build system (Plan 9 mk, not make)
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
- **sash-expand** — safe source-level replacement tool

---

## Build Requirements

- Rust (stable)
- Plan 9 `mk` (not `make`)
- System `ld` for linking

---

## License

Private — not yet licensed.
