# 486 — The path that was never looked at

2026-10-09. Continues 485.

## The question

DG read the behaviour-change flag at the end of the `waitstatus` work — a
spec that compiled `/nonexistant` and expected a runtime 127 — and asked:

> the /nonexistant should be caught at compile time, yes?

It should. It was not. The compiler had a three-way asymmetry nobody had
named:

| head | verdict |
|------|---------|
| `nonexistent_cmd` | **SB5011** at compile time |
| `./nonexistent_cmd` | **SB5051** at compile time |
| `/nonexistent_cmd` | **compiles**, 127 at runtime |

The absolute case — the only spelling DD#23 actively *encourages*, the one a
converted script ends up full of — was the one case the compiler declined to
check. `resolve.rs::is_resolvable_external` returned `false` for any name
containing `/`, so an absolute head was never looked at. The function that
would have answered the question, `resolved_path_exists`, sat directly beside
it, sysroot-aware and unused for this.

Worse, `--manifest` emitted `/nonexistent_binary_xyz` as a dependency. The
manifest is the compiler's statement of what the binary needs at runtime; it
was asserting, as fact, a requirement it already knew could not be met.

## The obstacle was inside the DD

DD#23's own text contained the one argument against fixing this — an example
headed "Or chain alternatives":

```sash
/usr/bin/grep pattern file || /bin/grep pattern file
```

which only parses as advice if an absolute path is allowed to be absent. I
put it to DG that the example is at odds with the DD's security spine: a
hand-written two-entry fallback *is* the search ambiguity DD#23 exists to
forbid, just spelled out by hand, and the DD already sanctions
`/usr/bin/env grep` for the portability case. DG approved the rejection:

> yes, reject any path that doesn't resolve (or is not executable -
> directory, device, mode 444, (maybe add a --target-user switch to do
> fine-grained access checking)); new code; let the spec harness optionally
> launch a test via a helper which orchestrates the enoent

The example is now withdrawn in an addendum rather than silently edited —
the DD said it, and the record should show that it did.

## SB5095

A literal absolute command head must name a **regular file with at least one
execute bit** in the resolution root: the build host, `--sysroot=<dir>`, or
`--sysroot=<image.squashfs>`. Every position is covered — top level, pipeline
stage, `$( )`, `&`, `-`, and an `exec` target — because they all pass through
`resolve_command`, and `exec` now delegates to the same authority.

The diagnostic names the fault rather than reporting a generic miss:

```
SB5095: command '/nonexistent' cannot be run: no such file
SB5095: command '/tmp' cannot be run: it is a directory
SB5095: command '/dev/null' cannot be run: it is not a regular file
SB5095: command '/tmp/ro' cannot be run: mode 0444 grants no execute permission
SB5095: command '/tmp/loop' cannot be run: its symlink chain does not terminate
```

Three codes for three distinct facts — bare is SB5011, relative is SB5051,
absolute is SB5095 — pinned by `cmdpath.15` and `.16` so a future
consolidation has to argue with a test.

**The only failure this rule cannot afford is a false reject**, so the
boundaries are explicit and specified. `[[ -x /path ]]` is untouched: DD#23
names it as *the* runtime existence test that replaces `command -v`, and a
compile-time rule that pre-empted it would delete the escape hatch the DD
promised. Globs are untouched: `head_is_unjudgeable` exempts `*?[$`.

## The squashfs rewrite was mandatory, not incidental

`unsquashfs -l` lists names only. Type and mode need `-ll`. That looks like a
small upgrade until you notice what the old code was accidentally right
about: `-l` recorded a symlink's path as present, so switching to `-ll` and
rejecting non-regular entries *without following symlinks* would have broken
every real image on earth — `/bin -> usr/bin` is the first entry in most of
them. The listing parser therefore follows links, with an 8-hop cap.

Four `-ll` parsing facts, each found empirically against a built image:

- a device node's size column reads `major, minor`, which breaks any
  fixed-column split; the path is located with `line.find(" squashfs-root")`
- file names may contain spaces, which the prefix search handles and column
  counting does not
- symlinks render as `path -> target`; split on `" -> "` only when the mode
  string starts with `l`
- the execute bit is a *character*, not a flag: lowercase `s`/`t` carries it,
  uppercase `S`/`T` does not, so `triple_bits` reads the character rather
  than testing `== 'x'`

## A wrong answer wearing a true answer's clothes

A symlink cycle initially reported `no such file`. That is defensible — the
path does not resolve — and it is exactly the shape this project keeps
deciding against. `waitstatus` returns `-1` on ECHILD rather than `0` for the
same reason: a plausible-looking answer that misdescribes the world sends the
reader somewhere useless. `PathMiss::{Absent, LinkLoop}` now separates them,
with ELOOP on the host branch and hop-limit exhaustion on the image branch,
so the message says what actually happened.

## The second bug, which was already there

Writing `resolved_path_executable` exposed a pre-existing fault in the PATH
search. `resolve_command_path` tested `is_file()`, so a present-but-unrunnable
candidate **won**: a mode-0444 `cptool` early in `PATH` shadowed the real
executable later in it, and the failure surfaced at `exec` time with the
shadowing path already baked into the binary. POSIX says keep searching. It
now does, and `cmdpath.17` holds it.

Two bugs from one question, which is the usual ratio when a check that should
have existed never did.

## The spec harness had to lose a capability to gain the rule

SB5095 costs the suite its only way to provoke a runtime exec-ENOENT: you can
no longer compile a program that names a missing absolute path, because that
is the whole point. Six specs depended on it — fewer than the thirteen I
estimated, because `/nonexistent_dir_55/f` and friends turn out to be
*redirect* targets rather than command heads.

`run_sash_vanish` restores the case honestly. It copies `/bin/true` to a
temporary path, substitutes that path for the literal `SPEC_VANISH_CMD` in
the source, compiles — and then `_vanish_now`, called in **both** backends
between a successful compile and the run, deletes the file. The spec is no
longer a program that names something absent; it is a program whose
dependency disappeared underneath it, which is the only way that runtime path
is reachable now and a truer description of when it happens in the field.

## Specs and gates

17 new specs, `cmdpath.1`–`cmdpath.17`, in `errors.sh` (plan 330 → 347).
Sweep **8031** against a prediction of 8031 — 7997 plus 17 × 2 modes — 34 new
ids, all `cmdpath`, **zero flips**. `mk test` green.

The complexity gate rejected four functions on the first pass
(`parse_squashfs_line` 12/3, `resolve_command` 10/9, `mode_from_listing` 7/6,
`host_path_facts` 6/6). All four came down by extracting named concepts
rather than by bypassing. `check_exec_target` then sat at cognitive 5 against
a 5.20 bar — passing by two tenths — so it was flattened as well; a function
that clears the bar by rounding error is one edit away from being someone
else's problem.

## Filed

`notes/tasks/target-user-access-check.md` — DG's parenthetical. `PathFacts`
records kind and mode but no uid/gid, and the squashfs listing reports owner
and group as *names*, so a real `--target-user` needs identity mapping before
it can answer anything. `PathFacts::is_executable` is the single predicate it
would refine.
