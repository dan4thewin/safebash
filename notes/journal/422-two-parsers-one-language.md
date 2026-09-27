# 422 — Two parsers, one language

The P1 audit of wildcard `match` arms found its first bug one layer above
`validate.rs`, in the parser, and it was not a wildcard at all.

## How it surfaced

The audit's stmt-level cohort had come back clean across all eleven
container shapes. Moving to the expression-level cohort, I needed a probe
program using `$(<file)` and wrote the spelling that comes naturally:

```sh
str f=/etc/hostname
echo "$(<$f)"
```

SB5050 — `f` assigned but never read. That is a dead-variable false
positive: `f` is read, one line down, as the filename. The dead-variable
gate runs last, so its firing is evidence that *nothing earlier counted
that construct as a read*.

`check_expr_reads` already has a `SlurpFile` arm, and its comment cites
this exact bug. The arm is correct. It was simply never reached, because
on this path the node was never a `SlurpFile`.

## What was actually wrong

A double-quoted string had **its own command-substitution parser**. Every
`"$( … )"` in the language went through `parse_sub_script` instead of
`parse_command_substitution`, and the two had drifted in the two ways two
implementations of one thing always drift.

**It had no `<` probe.** The real entry begins by peeking `Tok::Lt` and
routing to `parse_slurp_form`. The quoted one did not, so `"$(<f)"` was a
command substitution whose body was *empty*:

```
{:kind :command-substitution :body []}
```

`echo "$(</etc/hostname)"` printed nothing where bash prints the file.

**It could not carry a diagnostic.** Its signature was
`-> Vec<Statement>`, so the loop ended:

```rust
Ok(Some(stmt)) => stmts.push(stmt),
Ok(None) => break,
Err(_) => break,        // the diagnostic, discarded
```

DG asked the question that settled the shape of this: *shouldn't recursive
parsing catch the syntax error?* It does. `parse_statement` reported it
correctly every time. The caller threw it away. That reframed what I had
been describing as two bugs into one — a severed error channel — and it
is why the fix is convergence rather than two patches.

| program | was | bash |
|---|---|---|
| `echo "[$(</etc/hostname)]"` | `[]` | `[dangood-ubuntu-dev]` |
| `echo "[$(echo a; if)]"` | `[a]`, tail dropped, exit 0 | syntax error |
| `echo "[$(&&)]"` | `[]` | syntax error |
| `str v="$(<f)"` | compiles clean | — |

The last row is the one that matters. The unquoted `$(<f)` is SB5020:
file contents are external input. With an empty body there was nothing to
taint, so adding two quote characters laundered tainted content into a
clean-looking variable. A hole straight through the taint model, reachable
by typing the spelling most people would type first.

## Why it was one bug and not two

I initially proposed fixing the error channel and the `<` probe separately.
That is wrong, and the reason is mechanical: `<f` is an `Err` from
`parse_statement` exactly like `>f` is. Fixing only the channel would turn
`"$(<f)"` from a silent empty string into a *diagnostic* — still not a
slurp. The `<` probe has to come along, and once it does, the honest
version of the change is to stop maintaining a second parser at all.

DG chose that (option C) and settled the language question by principle
rather than by case: *both interpolation of tainted content and
concatenation model like a read, but a read of a tainted value can only
happen in `attest`.* So a quoted slurp is a real `SlurpFile`, tainted,
SB5020 outside `attest`, identical to the unquoted spelling. No new
surface — the surface was always supposed to be one thing.

## The fix

`command_sub_to_expr` now returns `Result` and delegates:

```rust
let src = format!("{})", content);
let mut sub = Scanner::new(&src);
parse_command_substitution(&mut sub, span)
```

The lexer strips the closing `)` while extracting the content, so it is
re-appended; the sub-scanner then sees exactly the token stream the
unquoted path sees after `$(`. `parse_sub_script` is deleted.

**The precedent was already in the file.** The `StringPart::ArithSub` arm
directly beside the call site does `parse_arith_str(…)?`, with a comment
recording that a malformed `$(( … ))` inside a double-quoted string used
to be swallowed into a literal `0`, so `"$(( 1 +* 2 ))"` printed 0 while
the identical text outside quotes was a hard error. Same bug, same file,
same shape, already fixed once. Reusing one parser is what stops it a
third time.

## What the specs say

Four new, in `misc.sh` (+8 dual-mode): `sl.12` the quoted slurp has
sl.1's byte-identical value; `sl.13` the taint gate reaches it; `cap.22`
a malformed quoted body is a diagnostic; `cap.23` a body that starts well
and then goes wrong is not silently truncated. Neutering the delegation —
restoring the old separate parser verbatim — fails exactly those four and
nothing else.

Sweep 6532, predicted 6532, zero failing, zero flips, eight new ids.

## A spec that was covering nothing

`cov.205` was the one casualty, and it deserved to be. It read:

```sh
# a `$( )` body that is a declaration, not a command: the rewrite's
# second guard. Quoted, because the unquoted path rewrites while
# scanning and never reaches this one.
run_sash '/bin/echo "$(int x=1)"'
[[ $rc = 0 && -z $out && -z $err ]]
```

Vacuous in both directions. It claimed the quoted path reached the
bareword-shortcut guard where the unquoted one did not — but the quoted
path had no shortcut probe *at all*, so it never reached it. And the clean
compile it asserted was not a property of the language; it was the
swallowed parse error. The unquoted spelling has always been SB4057.

Replaced with a body whose first token is a non-`Word` that actually
parses — `if` lexes to `Tok::If` — and an assertion on a value rather than
on silence:

```sh
run_sash 'str s="$(if [[ -e /proc ]] { /bin/echo hi })"
attest s [[ -n $s ]]
echo "[$s]"'
[[ $rc = 0 && $out = "[hi]" && -z $err ]]
```

A spec whose passing depends on a bug is worse than no spec, because it
converts the bug into a thing the suite defends.

## Filed, not fixed

`$(int x=1)` reports SB4057 "unterminated `$(`", which is misleading — the
paren is right there. It is pre-existing behaviour on the unquoted path,
verified against `HEAD` before the change; convergence merely made the
quoted spelling agree with it. Filed as
`bug-cmdsub-declaration-misreported-as-unterminated`.

## Method note

The audit did not find this by reading `validate.rs`. It found it by
running a probe program and reading the diagnostic, which is journal 400's
method note — *probe behaviour, not call sites* — paying out again. The
false positive was three passes downstream of its cause, and no amount of
staring at the wildcard arms would have reached it.
