# Session 2026-05-02 — DD#53 consequences + DD#54 source redesign

## Continued from session 048

While discussing DD#53 (env var isolation), examined how
`sash_source_kvp` actually works. Found that it pushes KEY=VALUE
lines into the process environment via `sash_setenv`, then the
codegen post-source loop re-reads them back via `sash_getenv` into
sash variables. This entire mechanism depends on implicit env-var
mirroring — exactly what DD#53 declares invalid.

## Source/KVP redesign discussion

DG observed that `source` coercion (`. ./vars` → `sash_source_kvp`)
should be replaced with an explicit pipeline using existing
primitives:

```bash
map config
sash-expand ./vars.conf | mapfile -m config
bless config [[ ... ]]
```

Key insights:

1. **`sash-expand` stays a real external binary.** It reads a file,
   evaluates the safe expansion subset (DD#1), outputs KEY=VALUE
   lines to stdout. Its output is tainted because it's an external
   command. This is correct — config files are external input.

2. **`mapfile` gains a map mode.** `mapfile -m varname` reads
   KEY=VALUE lines into a map instead of lines into an array. This
   is a natural extension of mapfile — same "read structured text
   into a container" pattern, different structure.

3. **`source`/`.` stays rejected.** No coercion. The user writes
   the explicit pipeline. This is more honest than pretending
   `. ./vars` works when the compiler replaces it with something
   fundamentally different.

4. **`sash_source_kvp` is removed from the runtime.** No special
   source handling in codegen. The mechanism is just a pipe to an
   external command + mapfile. No magic.

5. **`sash_setenv` calls in the KVP loader are eliminated.** The
   env var roundtrip (file → env → implicit sash var) is replaced
   by a direct path (file → stdout → map → bless). Aligns with
   DD#53.

## DD#54 — Source replacement: sash-expand pipeline

New design decision. Supersedes DD#01-09 item 2 (source compiled
away) and Decision 1 from the design review (source/. compiled
away, not rejected). See `notes/decisions/DD54-source-pipeline.md`.

Amends:
- DD#01-09 item 2: source/. is now rejected (SB4002 becomes a real
  error, not a coercion marker)
- DD#01-09 Decision 6: source and . still treated identically —
  both rejected

## Nameref discussion (conclusion)

Also resolved the nameref question from the DD audit. DG considered
the design review argument (namerefs are safe, compiler can resolve
bindings) against practical need:

- `foo=$(func)` is cheap in sash (no fork) — the bash performance
  motivation for namerefs doesn't exist
- Structured return types (arrays/maps) can use `mapfile` to
  recover structure from function output
- Globals work fine for output parameters in sash's compiled model
- Namerefs add lowerer/taint/SSA complexity for marginal ergonomic
  gain

Decision: keep SB5003 rejection. DD#01-09 item 3 updated with the
real rationale (unnecessary, not unsafe).

## Implementation tasks

Existing tasks affected:
- `remove-implicit-env-fallback` — also covers removing the
  post-source getenv re-read loop
- `remove-export-ssa-dead-code` — also covers removing
  `sash_source_kvp` codegen and `sash_setenv` from runtime

New tasks needed:
- `mapfile-map-mode` — implement `mapfile -m` for KEY=VALUE → map
- `source-rejection` — change SB4002 from coercion marker to hard
  error, remove source coercion from lowerer/codegen

## DD#23 rewrite — external command resolution

Revisited DD#23 while processing audit tasks `dd23-no-path-validator`
and `dd23-path-guide-contradiction`. The existing DD note said "PATH
is compile-time only" but the implementation (`sash_exec`) walks the
inherited PATH at runtime — neither of the journal's two modes was
implemented.

DG clarified: sash should never walk PATH itself. The design:

1. **Compile-time resolution.** Compiler resolves bare command names
   to absolute paths against the build host or `--sysroot`. Unresolved
   commands are fatal compile errors.

2. **Sash-native uses absolute paths.** No bare command names in
   sash-native source. `[[ -x /path ]]` for existence tests.

3. **`command -v` is coercion-only.** Bash `command -v foo` coerces
   to `[[ -x /resolved/path ]]`. No `command` builtin in sash-native.

4. **`env` is the escape hatch.** When a command genuinely can't be
   resolved at compile time, `env some-tool args` delegates PATH
   lookup to `/usr/bin/env` at runtime. Sash never walks PATH — it
   delegates to a program whose job is PATH search.

5. **Security model.** Sash binary + dm-verity squashfs = guaranteed
   code identity. Absolute paths + hash tree = auditable execution graph.

The journal's "Mode 1" (compiled-in PATH, runtime search) is eliminated.
The `env` escape hatch serves that use case more explicitly.

DD#23 rewritten. Previous status "implemented" changed to "designed" —
the implementation needs updating:
- `sash_exec` must stop walking PATH (just execve the absolute path)
- `command -v` runtime implementation must be removed
- Compiler needs a path resolution pass (bare name → absolute path)
- `--sysroot` flag needed for cross-compilation

## Implementation tasks created

- `dd23-remove-path-walk` — remove PATH search from sash_exec
- `dd23-remove-command-v` — remove command -v runtime implementation
- `dd23-compile-time-resolve` — compiler path resolution pass
- `dd23-command-v-coercion` — coerce bash `command -v` to `[[ -x ]]`

## Per-command env vars — delegate to /usr/bin/env

Further simplification: `FOO=bar cmd` syntax is also unnecessary.
`/usr/bin/env FOO=bar /usr/bin/cmd args` achieves the same thing
with zero env-var manipulation in the compiler or runtime.

This eliminates:
- Prefix-assignment parsing/lowering in the compiler
- Modified envp construction in codegen
- `sash_setenv` from the runtime (no remaining callers at all)

DD#53 updated: per-command env section now says "delegate to env."
`per-command-env-vars` task rewritten as a coercion (bash prefix
assignments → prepend `/usr/bin/env`).

The entire env-var story is now: sash never touches environment
variables, period. Reading requires `$(getenv NAME)` + bless.
Writing delegates to `/usr/bin/env`. The runtime has zero env
manipulation functions.
