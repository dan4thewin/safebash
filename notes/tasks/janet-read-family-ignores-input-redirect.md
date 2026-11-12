title: "janet: the read family ignores an input redirect and reads stdin instead"
status: deferred
deferred-by: DD104
priority: P2
kind: bug
depends: []
gates:
  - "janet: `read a < f` fills `a` from `f`, not from stdin"
  - "janet: `readline v < f` likewise"
  - "spec `errors.sh` 55.65 and 55.66 pass in janet mode (currently gated native,c)"

## What

```sh
printf 'alpha\0beta\0' > /tmp/f
read a < /tmp/f
attest x in a [[ -n $x ]]
echo "n=$(len a)"
```

native and c print `n=2`. janet prints `n=0` — the redirect is dropped and the
read takes the process's stdin, which the spec harness supplies as
`/dev/null`. The scalar form behaves the same way:

```sh
readline v < /tmp/f
attest v [[ -n $v ]]
echo "v=$v"     # native/c: the line; janet: empty
```

Both exit 0, so a mistyped path is silently an empty value — the same failure
mode that `read-family-redirect-open-not-guarded` fixed for native and c.

## Mechanism

The janet backend emits from the IR, not from SSA: `emit_janet.rs` handles
`read`/`readline` as *commands* (the `n == "read" || n == "readline"` arm), so
it never passes through the SSA `ReadArray` / `ReadLine` instructions that now
carry a `redirects` list. Nothing in that arm consults `c.redirects`.

## Found

2026-09-11, fixing `read-family-redirect-open-not-guarded`. The new specs
55.65/55.66 pin that a *successful* open still works; they surfaced this gap
rather than causing it. Verified pre-existing: the fix's `emit_janet.rs` diff
touches only the `match` out-array paths, and the probe above reproduces with
the read paths untouched.

## Note

Distinct from `c-janet-dd55-abort-channel` (janet has no abort channel for a
*failed* open) and from `janet-read-a-element-count` (a count discrepancy on a
pipeline, not a redirect). This one is about the redirect never being applied
at all. Likely subsumed by `emit-janet-from-ssa`.


## Deferred to post-beta (DD#104, 2026-09-12)

The in-tree janet backend is removed; `--emit-ir` replaces it with an
inert IR capture that an out-of-tree oracle can consume. This gap is
**not fixed** — it is recorded here as part of the diagnosis a restart
should read. The code it describes lives at tag `janet-babel`.
