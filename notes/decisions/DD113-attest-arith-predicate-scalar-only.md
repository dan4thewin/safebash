title: "DD#113 — the `(( … ))` attest predicate is scalar-only"
status: accepted

# DD#113 — the `(( … ))` attest predicate is scalar-only

DD#93 Part D gave `attest` a second predicate shape: instead of a `[[ … ]]`
test, a `(( TARGET = EXPR ))` assignment whose *computation* is the proof.
Converting a tainted string to an int is itself the validation, so

```sash
x=$(printf 123)
attest x (( y = x * 2 ))
```

declassifies `x` and binds a clean int `y`.

That form is **scalar-only**. There is no per-element conversion: a attest-all
(`attest e in a (( … ))`, `attest k v in m (( … ))`) has no single target to
assign, and `classify_attest` has always required a one-name lead for it.

The parser did not enforce this. `parse_attest` accepts `((` after *any* lead
— scalar or attest-all — because the `((` arm is checked after the lead is
already parsed. The resulting command decoded as no attest form at all, so
every attest-specific pass skipped it, and it fell through to the generic
command path as an external named `attest`:

```
$ sash prog.sh -o prog        # compiles clean
$ ./prog
sash: bug: bare command passed to sash_exec (expected absolute path)
prog: attest: command failed (exit 127)
```

An internal assertion reaching user output, for a program the compiler
accepted.

## Decision

A `(( … ))` predicate on a attest-all lead is **SB5054**, with wording naming
the actual constraint:

> attest-all takes a `[[ … ]]` predicate — the `(( TARGET = EXPR ))`
> conversion form is scalar-only

Same code as the scalar near-miss (both are "the `(( … ))` this attest carries
is not the form"), different message — the scalar text tells the author to
make `EXPR` read the attested name, which is not the problem here and cannot
be the fix.

Rejecting at the parser instead was considered and declined: `validate` is
where every other attest shape rule lives (SB5020, SB5053, the existing
SB5054), and splitting them across two passes would make the surface harder to
reason about than the one-armed `match` it costs here.

## Why it was open

`attest_arith_near_miss` — the function that produces SB5054 — carried this:

```rust
// Scalar lead only — `attest x (( … ))`; anything else is a different error.
let [only] = &c.args[..c.args.len() - 1] else {
    return None;
};
```

There was no different error. The comment describes a class of failure while
the code handles one instance of it — the same shape as DD#72, DD#64, DD#111
and DD#112. See journal 332.

An assignment-shaped predicate (`attest e in a (( t = e ))`) was caught, but
only incidentally: SB4020 rejects an assignment inside a non-statement
arithmetic position, and said nothing about attest. The non-assignment
spelling `(( 1 ))` had nothing to catch it.
