title: "Bug: in an if/elif/else body or an `&&`/`||` group, a latch read in the statement right after its write is a false SB5094 (`- cmd &` then `- wait $!`)"
status: open
priority: P2
kind: bug
depends: []
gates:
  - "every program in the first table compiles and runs as its top-level twin does, in both backends"
  - "`$?` keeps DD55's first-expression rule in those bodies: every SB5012 row of `bug-sb5012-message-names-no-command` keeps its verdict"
  - "the controls keep their results"
  - "a spec pins each row in both backends; each fails on the pre-fix binary"

## Found

2026-10-27, while filing `bug-sb5012-message-names-no-command` for F1+F4
(journal 537). Pre-existing at `8e28c61`, unchanged by F1+F4, and the same on
`taint-walker-redesign` (`e98fca8`); both backends.

`⏎` marks a newline.

| Program | Result |
|---|---|
| `if /bin/true { - /bin/true & ⏎ - wait $! ⏎ echo "[$(waitstatus)]" }` | SB5094 at 2:8 (`$(getcpid)`) and 3:6 (`$(waitstatus)`) |
| `if /bin/true { - /bin/true & ⏎ int p = $! ⏎ - wait $p }` (DD126's own example) | SB5094 2:9 |
| `if /bin/true { - /bin/true & ⏎ - wait ⏎ echo "[$(waitstatus)]" }` | SB5094 3:6 |
| `if /bin/true { : } else { - /bin/true & ⏎ - wait $! }` | SB5094 3:8 |
| `/bin/false \|\| { - /bin/true & ⏎ - wait $! }` | SB5094 2:8 |

Controls, all accepted:

| Program | Prints |
|---|---|
| `- /bin/true & ⏎ - wait $! ⏎ echo "[$(waitstatus)]"` at top level | `[0]` |
| `- /bin/true & ⏎ - wait ⏎ echo "[$(waitstatus)]"` at top level | `[0]` |
| `for i in 1 2 { - /bin/true & ⏎ - wait $! ⏎ echo "$i" }` | `1`, `2` |
| `fn g { - /bin/true & ⏎ - wait $! }` and `if /bin/true { g }` (the workaround) | — |

Probes: `files/port-f1f4-probes.tar.gz`, directory `t9/` (b5, b9-b15).

## Why it is a bug

DD126 (149-177): a latch read must be "in the statement **immediately
following** the one that wrote it", with `- /bin/sleep 1 &` then `int p=$!`
as the legal example. Constructs matter only at their boundary: "A `{ … }`
group is braces, not a catching construct, so the latch crosses it;
`if`/`while`/`for`/`case` and `&&`/`||` do not." Every row writes and reads
inside one body. The SB5094 text says the same ("the immediately preceding
statement"). As it stands, a job backgrounded in a branch cannot be waited
on by pid.

## Cause

`check_dq_body_branch` (validate.rs 3743-3754) checks each statement after
the first from `FlowState::new(StatusState::Undefined)` (3750). That is a
fresh state, so its latch is `Stale`. The reset implements DD55's
first-expression rule (DD55:43-47), which is about `$?` alone, and it takes
the latch with it.

Callers: `check_dq_if` (3395, 3401, 3404: then, elif, else) and
`check_dq_list` (3368, through `check_dq_stmt_branch` at 3732, for an
`&&`/`||` group). `check_dq_case`'s arms thread one state (3440-3443), and
so do loop bodies (the `for` control).

## Fix direction

Thread one state through the body and force only the status before each
later statement:

```rust
let mut st = state;
for (i, s) in stmts.iter().enumerate() {
    if i > 0 {
        st.status = StatusState::Undefined; // DD55: first expression only
    }
    check_dq_stmt(s, &mut st, diags);
}
```

Every read after a branch's first statement still meets `Undefined`, so the
SB5012 verdicts do not move. `bug-sb5012-message-names-no-command` gives
that `Undefined` a reason; either task can land first.

## Related: a `while` body does not reset `$?` (t4)

Found during F2 (journal 539); pre-existing on base, on the branch and after
F2, both backends. No spec pins the `while` shape.

| Id | Program | Result |
|---|---|---|
| t1 | `if /bin/true { /bin/true; echo "s $?" }` | SB5012 |
| t4 | `int n=0 ⏎ while (( n < 1 )) { /bin/true; echo "s $?"; n+=1 }` | compiles, prints `s 0` |

DD55:41-47 scopes `$?` to "the handling expression (`||` RHS, `if`/`while`
body)" and allows it only as the first expression evaluated there. Read
literally, t4 is a missed SB5012. The other reading (fc08d53's "original
status" wording) makes t1 and this task's first-table rows false rejections
instead. Either way `if` and `while` bodies must agree; whichever this task's
fix settles for `if`, the `while` body follows. `check_dq_body_branch`'s
callers are `check_dq_if` and `check_dq_list`, not the loop arms.

Probes: `files/port-f2-probes.tar.gz`, directory `sb5012/` (t1-t9).
