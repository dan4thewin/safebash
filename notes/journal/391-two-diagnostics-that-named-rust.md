# 391 — two diagnostics that named Rust instead of sash

`tok_to_text` led the cold list with 18 arms. It looked like pure test debt —
DD#116 had already made the match total, so the arms were presumed correct and
merely unvisited. Two of the three things found were bugs, and neither was in
the cold lines.

## The 18, settled by execution

Reading call sites would have got this wrong, so I instrumented instead: one
`eprintln!("TOKTEXT {:?}", tok)` at the head of the function, one build, a
sheet of shapes that puts each candidate token inside `(( ))`. That is a
single edit and it answers all eighteen at once, which is worth remembering —
the `panic!`-per-arm approach used for `attest` does not scale past two or
three.

| group | count | verdict |
|---|---|---|
| `(`, `)`, `((`, `))`, `$((` | 5 | **rival → `unreachable!()`** |
| `Comment`, `Eof` | 2 | **backstop, keep** (DD#116 property 3) |
| `{`, `}`, `[[`, `]]`, `>&`, `<&`, `<>`, `&>`, `'…'`, `"…"`, `$(` | 11 | reachable, untested → shields |

The five paren arms are unreachable because `step_arith_collect` intercepts
them *before* `step_default_token`, the sole caller. They are a rival in the
DD#116 sense, and — this is the part that decided it — a rival **already out
of step**: the interceptor's rendering is depth-sensitive (`))` emits nothing
when it closes the expression, a lone `)` may terminate collection), while
`tok_to_text` would have emitted the text unconditionally. Two answers to one
question, silently disagreeing. They become `unreachable!()`, which keeps
property 2 (a new token is still a compile error here) without offering the
second answer.

`Comment` and `Eof` are *not* rivals — nothing else answers "what text does a
comment become inside arithmetic?" — and `Comment => String::new()` states
DD#94's rule (comments are trivia inside `(( ))`) rather than the accident
that the scanner currently strips them. Property 3, keep.

**DD#94 also stopped a false positive.** `(( x = 2 # hi ))` gives 2 in sash
and a syntax error in bash, which has exactly the shape of the silent-drop bug
the function's own doc comment describes. It is not one: DD#94 makes comments
and newlines trivia inside `(( ))` and `[[ ]]`, and records that bash rejects
these cases too, "so this is a **deliberate divergence**". Checking
`notes/decisions/` before believing a divergence is a bug is the rule that
earned its keep here.

## Bug 1 — SB4020 named a Rust variant

```
$ sash -e '(( x = 1 >& 2 ))'
error [SB4020] expected an operand in arithmetic expression, found Amp
```

`Amp` is the `ATok` enum variant. The user typed `&`. The message was built
with `format!("… found {tok:?}")`, so every malformed-operand complaint in
arithmetic reported an internal name — `Star`, `Gt`, `Caret`, `PipePipe` —
for a character the user had written.

A second site did the same: `expect()` rendered `expected {:?}`, so a ternary
missing its colon said `expected Colon`.

Fixed with `atok_to_text`, a total match over all 45 `ATok` variants. Total,
not `_`-defaulted, for the same reason `tok_to_text` is: a new token must be a
compile error here rather than another leaked name.

Found by **reading the output of the shields I was adding**, not by looking at
a cold line. The eleven reachable arms all reject, and I was checking the
rejects were sane before writing them into specs.

## Bug 2 — a second answer to "how is a keyword spelled?"

`expect_word` handles keywords used as identifiers (`for in in a b`). It
derived the identifier text like this:

```rust
let name = format!("{:?}", scanner.peek().token).to_lowercase();
```

`keyword_to_str` is documented as the **only** list of reserved-word
spellings, precisely because the one time that list was duplicated the copies
drifted, `Tok::Str` went missing from both, and the bare word `str` failed to
lower in every word context (journal 347). This is a third copy, derived from
Rust identifiers rather than written down — agreeing only by the accident that
all eight variants in the arm happen to be single lowercase words. It would
have produced `dblbracketopen` for a variant whose name is not its text.

Now calls `keyword_to_str`. Behaviour identical (`fn in`, `fn do`,
`for in in a b` all verified unchanged), which is the point: a rival in
agreement is invisible until it isn't.

## Measured and deliberately left

`(( 1 [[ 2 ))` reports "``$a[i]`` isn't a subscript read in arithmetic". The
advice is wrong for that input — the user wrote `[[`, not `$a[i]` — but the
hint is a documented heuristic for the common case
(`arith-bare-subscript-followups` edge 1) and only misfires on input that is
nonsense anyway. Changing it risks the case it was built for. Noted, not
touched.

`DoubleQuoted` renders as the placeholder `"..."` rather than its parts. Lossy,
but safe in a way worth writing down: `"` is never valid in sash arithmetic, so
the shape always rejects, and SB4035 names the real `"` rather than the
fabricated ellipsis. Rendering the parts faithfully would mean re-entering the
display-vs-semantic trap that made `argv "1" a` ICE (journal 389).

## Gates

15 specs, `31.75`–`31.89`. Two of them (`31.86`, `31.87`) assert the *absence*
of `Amp` / `Colon` as well as the presence of the right spelling — a
regression to `{:?}` would still contain the word "found", so the negative
half is the shield that actually bites.

`mk test` 51/27/20. Sweep **6041**, the predicted total (+2×15), 0 failing,
0 flips. Three id checks clean; source-derived total agrees exactly.
