# 451 — The same rule, applied to the same construct

`q-cmdsub-body-taints-outer-global` had been open since 29 September. DG closed
it today in two moves, and the second one is the interesting one.

## The question

A **function** body that leaves a global tainted is a located `[SB5067]`. A
`$( … )` body doing the same was silent.

Not because the write does not happen — sash inlines a `$( )` body rather than
forking a subshell, so the write escapes and takes effect. DD#86 already says
so. The taint simply did not follow the value out.

The filed example understated it, because it used a *clean* write:

```sash
str t=clean
str q=$(t=modified; /bin/echo x)
```

With a real taint the question is two lines:

```sash
str t=clean
/bin/echo $t
str q=$(t=$(getenv HOME); echo x)   # compiled silently
```

```sash
str t=clean
/bin/echo $t
f() { t=$(getenv HOME); }            # [SB5067]
```

Identical semantics. One diagnosed, one not. The global has to be read *before*
the body for the shape to be visible at all, otherwise SB5050's dead-variable
gate fires first and hides it — the same masking that has now bitten three
separate investigations.

## The first move: measure what the relaxation had already done

The task was filed before DD#121's net-effect relaxation landed. Its stated
cost for option (a) — extend SB5067 to bodies — was that it "would newly reject
programs that taint inside a body and attest after it".

That cost was gone. Under the net-effect rule an adjacent **in-body** attest is
the repair, and it already worked:

```sash
str t=clean
str q=$(t=$(getenv HOME); attest t [[ -n $t ]]; echo x)
/bin/echo $t                          # prints the value; the attest is a real
                                      # runtime gate, not a compile-time waiver
```

So (a) would newly reject exactly one shape: the genuinely leaky one that the
function rule already rejects.

## The second move: DG deleted the exception I had flagged

I had written that the anonymous-function model settles most of the question
but leaves `:!` open — a global a `$( )` body tainted, read later as `$t:!`,
compiles and leaks. I listed it as something the model defers rather than
answers, and cited DG's own note from the DD#121 discussion: "the sole exception
being a missing attest and a global only accessed by `:!` (an exception we
could disallow...)".

DG's reply: *":! no longer can happen, since a function can't leave a global
tainted, yes?"*

He was right and I was wrong. SB5067 under the net-effect rule fires at **body
exit**. It never looks at the read site. So the `:!` is evaluated at a statement
the rule has already failed before reaching:

```sash
str t=clean
/bin/echo $t
f() { t=$(getenv HOME); }
f
/bin/echo $t:!                        # still [SB5067]
```

Which means the exception was not an open question at all — the relaxation had
closed it as a side effect, weeks before anyone asked. And it closes for
anonymous bodies for the same reason, so the model settles the question
*completely*.

What survives is what DG meant by "within the same scope": `:!` still reads a
**top-level** taint, and a body's **own locals**. Neither is a body exporting
taint, so SB5067 has no jurisdiction over either.

I had reasoned about `:!` as a property of the read, when it is the *rule* that
is the body-exit property. Worth remembering: when a rule and a bypass seem to
interact, check which one the rule actually examines.

## The implementation

`taint_cmdsub_bodies` now runs the same three steps as `taint_function_body` —
collect sites, snapshot the entry set, walk, pass the verdict. Eight lines.

Two details worth recording:

**Exactly one report.** A `$( )` inside a function body is reported once, by the
body. `collect_global_taint_sites` is structural, so the enclosing function does
not gather the same site; and the cmdsub's clone-and-discard keeps the taint out
of the function's exit set, so the function's own verdict sees nothing. Nested
`$( )` bodies behave the same way, each reporting its own. Pinned by `bfb.28`.

**The diagnostic's subject.** A function passes its name; an anonymous body has
none, so it passes the construct (`$( … )`) and lets the span do the locating.
The rationale clause had to generalise — "a function may not export taint to its
caller" became "taint may not escape the body that creates it" — which is true
of both. Every existing SB5067 assertion matches on `leaves the global` or
`assignment`, so none needed changing.

## What it cost

Nothing. Sweep 7071 → 7083, the entire +12 being the six new specs. No existing
spec changed verdict, which is the empirical form of the prediction that (a)
newly rejects only a shape already rejected elsewhere.

## What it bought beyond the diagnosis

The task's real argument was architectural. Cloning the tainted set per body is
sound for functions "precisely because SB5067 makes the top level the only place
a global's taint can arrive" — and `taint_top_level` documents the consequence:
"the set this leaves behind is final — no fixpoint". That invariant did not hold
for `$( )` bodies, so `taint_cmdsub_bodies`' clone-and-discard was, strictly, an
assumption.

It is now a theorem. The verdict is what makes the discarded set provably equal
to the one the enclosing scope keeps. Option (b) — merging taint outward — would
have bought precision at the price of that invariant; (a) buys the invariant.
