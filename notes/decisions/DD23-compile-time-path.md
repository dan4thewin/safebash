title: DD#23 — External Command Resolution
status: partial

Sash does not walk PATH at runtime. External commands are resolved to
absolute paths at compile time.

## Compile-time resolution

The compiler resolves bare command names to absolute paths:

    grep → /usr/bin/grep

Resolution searches the build host or a target sysroot:

    sash compile script.sh                          # build host
    sash compile --sysroot=/mnt/target script.sh    # target root
    sash compile --sysroot=image.squashfs script.sh # squashfs image

Unresolved commands are fatal compile errors.

## Absolute paths in source

Sash-native scripts use absolute paths directly:

    /usr/bin/grep pattern file

Or chain alternatives:

    /usr/bin/grep pattern file || /bin/grep pattern file

## Existence testing

`[[ -x /path/to/binary ]]` replaces bash's `command -v`. No
`command` builtin in sash-native — it's a coercion target only.

## Coercion from bash

    # bash:
    if command -v docker >/dev/null; then docker build .; fi

    # sash (coerced):
    if [[ -x /usr/bin/docker ]]; then /usr/bin/docker build .; fi

The path comes from compile-time resolution. `command -v` is
eliminated; `[[ -x ]]` tests runtime existence.

## The env escape hatch

When a command genuinely can't be resolved at compile time (not on
the build host, runtime-only tool), use `env`:

    /usr/bin/env some-tool args

`env` is itself resolved at compile time. It performs PATH lookup at
runtime. The delegation is explicit, visible, auditable. Sash never
walks PATH — it delegates to a program whose job is to walk PATH.

## Manifest

In resolve mode, the compiler emits a dependency manifest listing
every external binary the compiled script requires.

## Security argument

When the sash binary runs atop a dm-verity squashfs, the absolute
paths in the binary + the hash tree guarantee exactly which code
executes. No PATH injection, no symlink tricks, no search ambiguity.

## Runtime

`sash_exec` takes an absolute path only. If the argument contains no
slash, that's a compiler bug — the runtime should not attempt PATH
search.

## See also

- DD#53 — Env var isolation (PATH not readable unless via getenv+bless)
- DD#45 — Export rejected (can't modify inherited PATH)

## Clarification (DG, 2026-06-21)

- **No bare externals reach the back-end.** Every external is mapped to an
  absolute path; this resolution must run **ahead of the front-end checks**
  (validate/taint/type-check), not in the transform as it does today. See
  `advance-dd23-resolution`.
- **Taint source = `execve(2)`/`execveat(2)`.** Any captured output that
  passed through an exec is tainted (e.g. `printf`, `pwd` — real programs).
  In-process internal builtins (echo, getcwd, getpid, …) and inlined user
  functions are clean; `getenv`/`argv`/`arg` are tainted external input.
  See `tighten-cmdsub-taint-execve`.
- **--sysroot incl. squashfs** is part of resolution (build host default,
  or a target dir / squashfs image root).

## Addendum (2026-09-03) — relative external paths are rejected (SB5051)

DG hit two coupled gaps while running an arg-parser with
`PATH=.:$PATH ./sash 17` (a `sash-getopt` capture). `sash-getopt` resolved to
the **relative** path `./sash-getopt` (a `.`-in-PATH match), and:

1. The CST taint pass (`command_name_is_taint_source`) detected externals with
   `name.starts_with('/')` — **absolute only** — so the relative-path capture
   was **not** flagged. Only the fail-closed SSA taint backstop (basename-based)
   caught it, producing an "SSA taint backstop" reject where a clean **SB5020**
   should have fired. That divergence is the symptom; the root is that a
   relative external path exists at all, which DD#23 forbids.

**Rule (reaffirming DD#23's "absolute path only / no search ambiguity"):**

- **Resolution never yields a relative path.** `resolve_command_path` searches
  only **absolute** PATH entries; a relative dir (`.`, `..`, an empty entry =
  cwd) is skipped. A command found solely via a relative dir is unresolved →
  **SB5011**.
- **A source-written relative external head is a compile error — SB5051.** A
  literal command head that contains `/` but does not start with `/` (`./x`,
  `../x`, `a/b`) is rejected uniformly in every position (top-level, `$( )`,
  pipeline stage). An **absolute** `/x` path is unaffected; a bare unknown name
  is still "not found" (**SB5011**), which is distinct from a relative-path
  error.
- **CST taint hardening (defense-in-depth).** `command_name_is_taint_source`
  now uses `contains('/')` (not `starts_with('/')`), so a relative external
  would be tainted even if one ever slipped the rejection. The SSA backstop was
  already fail-closed (basename → tainted unless a known clean builtin/user fn).

Impl: `resolve.rs` (`resolve_command_path` dir-skip; `resolve_command` SB5051;
`is_valid_command` defers relative heads so `resolve` owns the diagnostic),
`validate.rs` (`command_name_is_taint_source` → `contains('/')`). Specs
`errors.sh` relpath.1–6 (×3 modes). SB5051 is a new code.
