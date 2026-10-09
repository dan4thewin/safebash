# 487 — The parser that nothing had ever parsed with

2026-10-09. Continues 486.

## What the coverage queue was for

`coverage.db` was stale — 19 measured files renumbered since it was built, so
every cold-line row pointed at whatever now occupied that line. Rebuilt it,
ranked by function, and the head of the list was **code I had landed an hour
earlier**: six functions in `lib.rs`, 42 cold lines, the entire `unsquashfs
-ll` listing parser and symlink follower from the SB5095 work.

I had verified all nine squashfs cases by hand against a built image and said
so in journal 486. That verification was real and it was worth nothing the
moment the session ended. Nothing mechanical held any of it.

## Why the existing specs did not cover it

`cli.31`/`cli.32` have tested the squashfs branch for a long time. They point
`--sysroot=` at a **nonexistent** `.squashfs`, so `unsquashfs` prints nothing,
`.lines()` yields nothing, and `filter_map(parse_squashfs_line)` **is never
called once**. The checks assert the right end-to-end behaviour and give the
parser zero coverage. I had read them as "the squashfs branch is specced"
without asking what they reached.

They also run with `PATH=/bin`, which I expected to make `unsquashfs`
unfindable and so exercise the `Err(_)` arm. It does not: `/bin` is a symlink
to `/usr/bin` on this box and an apt `unsquashfs` from 2021 lives there,
distinct from the 4.6.1 that `which` reports. The comment in the spec file now
says not to add `PATH=/bin` to the new block, because restricting it makes
every check pass for the wrong reason.

## The bug it found: a false reject

Building the fixture turned up a case I had not tried by hand — a symlink
whose target contains `..`:

```
/usr/bin/good -> ../lib/realtool        (and /usr/lib/realtool is executable)
```

The host agrees it is valid and runnable. SB5095 rejected it:

```
error [SB5095] '/usr/bin/good' is not an executable program — no such file
```

A **false reject**, which journal 486 had named, two days of work earlier, as
the one failure SB5095 cannot afford. And `../lib/...` is not an exotic
spelling — it is how real images spell most cross-directory links.

The follower joined the link's directory to the target and looked the result
up as a literal key. A squashfs listing contains no `..` in any entry, so
`/usr/bin/../lib/realtool` matches nothing. `normalize_path` now resolves `.`
and `..` lexically before the lookup, clamping at the root as the kernel does.

**The first fixture I wrote for this said the opposite.** `/usr/bin/dotdot ->
../bin/realtool` with the target at `/bin/realtool` looked like the same case
and the compiler rejected it — but `..` from `/usr/bin` is `/usr`, so that
link really did dangle and the rejection was correct. I nearly "fixed" a
non-bug. What separated them was checking the fixture against the host
(`[ -x ... ]`) instead of against my expectation of it. Both are now specs:
`sq.4` is the valid link, `sq.5` the dangling one, and `sq.5` exists
specifically so that `sq.4` cannot be satisfied by a follower that stopped
checking.

## Fifteen specs, and what each one holds

The fixture needs no root. `mksquashfs`'s pseudo-file syntax supplies a device
node and `chmod` supplies every mode case, so the whole matrix is buildable by
a normal user.

The checks worth naming are the ones that record a fact about `unsquashfs -ll`
output that nothing else in the tree records:

- **`sq.8`, the device node.** Its size column reads `1,  5` — two
  whitespace-separated fields where every other line has one. A
  column-counting parser reads the path from the wrong place on exactly this
  line and nowhere else.
- **`sq.10`/`sq.11`, the setuid pair.** `-rwSr--r--` must come out mode 0644
  and `-rwsr-xr-x` must come out 0755. Uppercase `S` is setuid *without*
  execute; lowercase `s` is setuid *with* it. This pair is the whole reason
  `triple_bits` reads the character rather than testing it for `x` — testing
  for `x` would false-reject every setuid binary in a real image. `sq.12` adds
  the sticky `t`, so a fix that special-cased only `s` still fails.
- **`sq.6`, the cycle.** Named as a loop rather than reported absent, which is
  what `PathMiss::LinkLoop` is for.

`sq.15` closes the manifest loop: SB5095 exists so the manifest cannot assert
a dependency the compiler knows is unmeetable, and `sq.15` is the other half
— what it *does* assert comes from the image.

## What a spec could not reach

A path containing a space is a real `-ll` format fact — the parser locates the
path by prefix search precisely so a spaced name survives — but no spec can
reach it. A command head with a space is rejected by SB5011 long before
resolution, so the listing line can never be the thing under test. That, the
`" -> "` separator applying only to `l` modes, the image root keying the empty
path, and the short-mode-string guard all became unit tests in `lib.rs` with
verbatim sample lines.

The guard one is worth its place: `triple_bits` indexes bytes 1..10, so a
malformed mode string would be an out-of-bounds **panic** — the compiler
crashing on whatever output an `unsquashfs` we do not control decided to
print.

## A cache with no key

`SQUASHFS_CACHE` was a `static Option<HashMap<…>>` populated on first use and
never keyed by the image it came from. A second compile against a *different*
image in the same process would have been answered from the first image's
listing. One compile per process is all the CLI does, so it was a trap rather
than a live bug — and the doc comment claimed "read once per compile" when the
truth was "per process". Keyed it, which costs nothing and makes the comment
true, rather than writing the trap down.

## Numbers

Sweep **8061** against a prediction of 8061, 30 new ids all `sq.*`, **zero
flips**. `mk test` green, 40 invariant tests. Cold lines **458 → 416**,
uncovered functions **98 → 92** — the whole 42-line block, warmed by spec.

Two lines in it remain cold and only one is work:

- `mode_from_listing`'s short-string guard — now held by a unit test, since
  no listing a real `unsquashfs` produces can reach it.
- `squashfs_listing`'s `Err(_)` arm, which fires only when `unsquashfs` is
  not installed at all. No spec on a machine where the feature works can
  reach it. See below.

Format is byte-identical between `unsquashfs` 4.4 (2019) and 4.6.1 (2023), a
four-year span, which is the best evidence available that the parser is not
version-fragile.

## Open for DG

An image the compiler **cannot read** — `unsquashfs` absent, or the image
corrupt — yields an empty listing, and every path in it then reports SB5095
*"no such file"*. The doc comment presents this as deliberate, justified by
`--sysroot=<dir>` giving the same answer for a missing root.

With SB5095 now naming the fault, I think that reads as the same wrong answer
in a true answer's clothes that `PathMiss::LinkLoop` and `waitstatus`'s `-1`
were both separated out to avoid: the user is told their paths do not exist
when the compiler never managed to look. Raised with DG rather than changed,
since the comment says it was decided.
