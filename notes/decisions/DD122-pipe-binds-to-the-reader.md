title: "DD#122 — a pipe binds to the reader, not to fd 0"
status: accepted
date: 2026-10-03

# DD#122 — a pipe binds to the reader, not to fd 0

## The rule

DG, 2026-10-03:

> `readline` or `while readline` without a leading pipe would read fd 0;
> `cmd | while readline v` would read the output of `cmd`; a `/bin/cat`
> within that `cmd | while` would read fd 0.

Three clauses, one rule: **a pipeline delivers its data to a named reader,
not to a file descriptor.**

| what | reads |
|---|---|
| `readline v` / `read a`, no pipe | fd 0 |
| `cmd \| readline v` / `cmd \| read a` | the pipe |
| `cmd \| while readline v; do … done` | the `readline` reads the pipe |
| a command *inside* that loop body | fd 0 |
| `cmd \| /bin/cat` (a forked stage) | the pipe, as fd 0 |

The last two lines are the ones that look inconsistent and are not. A
pipeline *stage* is forked, so it inherits the pipe as its stdin, exactly as
bash does. A command in a reader-loop's **body** is not a stage — the loop
runs in the parent, so the command inherits the parent's stdin. Nothing
rebinds fd 0 in the parent, ever.

## Stage arguments and attached redirects (DG, 2026-11-07)

**Arguments first** applies here too (DD86, journal 585). A forked stage
establishes its pipeline descriptors, evaluates its arguments in that
child, then processes its own attached redirects and runs the command.
Argument captures can therefore read pipeline-fed stdin, but not the
command's later `< file`. Their own capture routing and inner redirects
keep their scopes; an outer active redirect is still inherited.

Pipeline plumbing is not an attached command redirect. This ruling does
not move argument work into the parent, rebind a parent reader's fd 0 or
change lexical reader ownership. Enforcement remains pending in
`q-stage-capture-redirect-order`; the present stage implementation still
installs attached redirects before argument captures.

## Why the loop runs in the parent

This is the whole reason the rule exists.

```sash
int n=0
/bin/printf 'a\nb\nc\n' | while readline v; do (( n = n + 1 )); done
echo "n=$n"
```

**sash prints `n=3`. bash prints `n=0`**, because bash makes every pipeline
stage a subshell and the increments die with it. That is bash's most
notorious gotcha, and safebash does not reproduce it: the loop is lowered
into the parent and the pipe is handed to the `readline` inside it.

Keeping that property is what forces the rest of the design. The loop cannot
be forked, so fd 0 cannot be redirected to the pipe — a parent-wide fd
change would leak to every later statement. The pipe therefore has to attach
to the thing that reads it.

## Consequences

### 1. A reader binds to its own pipeline and no other

The read-family emitters take a lexical binding from `VersionState::pipe_reader`,
installed only while lowering the pipeline's final stage. A negative fd encodes
its owned reader site; codegen resolves that site's record, not an ambient fd.

The original post-lowering repoints walked **every block lowered so far**,
reaching backwards:

```sash
/bin/printf 'x\n' | while readline a; do echo "A=$a"; done
readline mid                       # no leading pipe — must read fd 0
echo "MID=[$mid]"
/bin/printf 'y\n' | while readline b; do echo "B=$b"; done
```

`MID` came out **empty**, aimed at the first loop's exhausted pipe. Delete
the first pipeline and the identical statement reads fd 0 correctly. Exit 0,
no diagnostic, both backends.

Scoping those scans to newly lowered instructions was not sufficient. They
still stole reads from inlined functions, command substitutions and redirected
commands, and the loop scan missed bulk `read` entirely.

Binding now happens when each reader is emitted, not in a later SSA scan.
Nested pipelines save/restore the lexical binding; calls, captures, redirected
commands and background children suspend it. A reader in a called function
uses that function's stdin, and `< file` keeps meaning the file. A bulk read
in a bounded loop body binds just as a scalar `readline` does. The old scan
helpers are removed.

### 2. Every reader owns its pipe and pending children

The first scoping repair still kept one ambient captured fd. An inner reader
overwrote it: an outer `one\ntwo\n` pipeline with an inner reader printed only
the `one` iteration, then mistook the inner pipe's EOF for the outer's.
Each reader now has a unique site, private fd and pending-child array in both
backends. Its negative fd encodes that site, including through nested calls.
The runtime record uses `-1` for an absent or closed pipe; fd 0 is never an
absence sentinel.

DG, 2026-10-18:

> why wouldn't each consumer simply keep doing reads until EOF or sigpipe,
> and only then evaluate child status?

**Start producers, consume concurrently, close the reader, wait, then evaluate
producer status.** Waiting first deadlocked once output filled a pipe. Reader
effects before a producer's eventual failure are normal streaming effects,
not a reason to spool or postpone consumption.

A single `readline` finishes after its record; `read` drains to EOF. A loop
consumes until its condition finishes or control leaves it. Normal completion,
`break`, escaping `continue`, `return` and tail calls close and reap the scopes
they leave, inner first. Ordinary loop continuation retains its reader.
Closing an early consumer may give its producer SIGPIPE; DD#55's existing
per-stage suppression and aggregate-status rules still apply.

Terminal dispatch to EXIT or ERR closes all outstanding readers before reaping,
preserving the triggering status. A forked child closes inherited reader fds
without adopting its parent's children. An authored `wait` may reap an owned
producer; its status is retained for the owning pipeline's later aggregate,
without changing the public wait result. No path force-kills producers or adds
a forced termination timeout.

### 3. The final stage must consume the pipe (SB4022 / SB4073)

If the pipe binds to a reader, a final stage with no reader receives
nothing. Two shapes could reach that state and both are now rejected at
parse time.

**A compound final stage — SB4022.** `cmd | { … }`, `| if`, `| for`.
These *do* read the pipe, because they are forked, but a fork is exactly
what the parent-process lowering exists to avoid:

```sash
/bin/printf 'a\n' | { readline g; }          # ran, read a line, g unset
/bin/printf 'a\n' | if readline g; then … fi # took the branch, g unset
```

The read succeeded, the branch was right, the data silently vanished. That
is the bundling **DD#38** rejects `( … )` for — `( readline g; )` is SB4014
in every position — arriving through a different syntactic door. It now gets
DD#38's answer and SB4022's existing advice: *wrap it in a function*, which
is a legal stage and is byte-for-byte equivalent in bash too.

A non-final compound was already SB4022 (the `|` parses as a postfix on the
compound). This completes the rule to both positions.

**A `while` with no reader — SB4073.** `cmd | while (( n < 1 )); do
/bin/cat; done` ran the loop in the parent, and nothing bound the pipe, so
the producer's output went nowhere. It is consistent with the rule and is
almost certainly a mistake, so it is an error rather than a silent discard.

The transform's `is_while_read` stays structural — `matches!(StmtKind::
While(_))` — because SB4073 now guarantees what it used to assume.

### 4. Legal final stages, exhaustively

- an ordinary command — forked, inherits the pipe as fd 0
- a user function — the same, inlined as a forked compound child
- `readline NAME` — DD#99 scalar lastpipe, parent
- `read ARRAY` — DD#99 NUL-split lastpipe, parent
- `while` — parent, with a `readline`/`read` in its condition or body

The reader search (`Statement::binds_a_pipe_reader`) deliberately does not
descend into a nested `Pipeline`, a `Function` definition, or a `Redirected`
body: a reader in any of those binds to something else, and the transform's
lexical context follows those boundaries.

## Notes

The rule was implemented before it was written down. The pipe-attachment
defects above were all found by reading the code and inferring the design
from it, which produced the wrong model twice — once concluding that a
`/bin/cat` inside a reader loop *should* see the pipe. DG's three clauses
settled it. This DD exists so the next reader does not have to guess.

## Addendum (2026-10-26): what a stage may be, and the final reader's marks

§4 lists the legal final stages. It did not say what an earlier stage may be,
or which marks the final reader may carry. SB4022's principle settles both: a
stage runs in a forked child, so a builtin whose effect lands on the process
that runs it would act on the child, and the effect would be lost.

**An in-process builtin is SB4022 as a stage, in any position, the last
included (batch-531 B12).** That means `return`, `break`, `continue`, `exit`,
`cd`, `wait`, `close`, `unset`, `signal`, `argv`, `keys`, `match` and
`matchall`, plus `read`/`readline` anywhere but last. (`attest`, like
`[[ … ]]`, was already SB4022 as a statement form.) Nearly all of them
compiled and died at run time with "bug: bare command passed to sash_exec".
`argv`, `match` and `matchall` were native ICEs. The rest drew another code:
`keys` a false SB5050, an earlier `readline` SB5020, an earlier `read` SB5069.
`exec` as a stage is SB4022 too, with a message of its own: the stage is
already a forked child, so `exec` would replace or rewire that child, not the
program. `echo`, `write`, `:`, an external command and a user function stay
legal stages. The classifier is `ssa::BuiltinCmd::acts_on_its_process`. It is
total over the enum, and the `&` gate shares it (DD#55 addendum 2026-10-26).
B12 named the control words. The rest of the class is decide-and-report.

An assignment stage is now called "a variable assignment" in the message. It
used to be called "`(( … ))`" (B19).

**The final reader beneath its marks.** The reader is the last stage whose
statement, beneath its `-`/`!` marks (`Statement::stage_marks`), is a
`readline` or a `read`.

- `a | - readline x` and `a | - read ar` are legal. DD#55's `-` runs the
  reader unchecked, and its verdict does not join the producers' status, so a
  condition's verdict is the earlier stages' alone (decide-and-report).
  `- readline` died with the bare-command bug natively, and gcc rejected its
  C output. `- read` was a native ICE, and its C binary printed a garbage
  length.
- `a | ! readline x` and `a | ! read ar` are legal where a condition or a
  return producer consumes them. The reader's verdict is inverted before it
  joins. In a condition `! read` was a native ICE, and its C binary printed a
  garbage length. At statement level a negated reader is SB4018, as is any
  terminal `!` that nothing consumes (DD#70 addendum 2026-10-26).
- A redirect on the final reader is SB4022 (decide-and-report). The reader
  reads the pipe in the parent, so the redirect has nothing to attach to, and
  no backend lowered it. On `readline` the native binary died of the
  producer's SIGPIPE and gcc rejected the C output. On `read` native ICEd, and
  in C the producer died of SIGPIPE. So `stage_marks` peels only the `-`/`!`
  marks, while `stage_body` also peels a redirect for classifiers that ask
  what a stage runs.
- A `while` final stage may carry `-` but not `!`, which is SB4022
  (decide-and-report). The loop's own status never joins the pipeline's, so
  `!` would have nothing to invert. Both forms used to ICE. This SB4022 and
  SB4073 (a last-stage loop that never reads the pipe) name the loop the
  user wrote: SB4073 said `while` for an `until`.

A mark on a reader no longer changes its taint (DD#24 addendum 2026-10-26).

Specs: pipe_functions.sh pf.35-pf.68 (pf.52, `a | b &`, is DD#55's);
coverage.sh cov.182 (now pins SB5069 with `&&`, since `read A | read B` is
SB4022 by pf.46); taint.sh 24.22-24.25, 24.28, 24.29. Journal 537.
