title: Clean repo root and harden .gitignore
status: done
priority: P5
kind: work
source: opus-47-review
resolved: 2026-06-05
resolution: Removed 14 stray artifacts from repo root (a.out, dangling-link, fifo-file, file.txt, link, out.txt, output.txt, stderr.txt, stdout.txt, test-dir, test-file, test.txt, valid-link, and the literal ~/work/lsg-talk/ dir). All but fifo-* were already gitignored (just untracked debris); added fifo-* glob to .gitignore for completeness. Used explicit './~' addressing for the tilde-dir to avoid accidental $HOME expansion.
landed: 2026-06-05
depends: []
gates:
  - git status clean after the change
  - mk test still passes
  - mk clean removes nothing it shouldn't

Stray artifacts in the working tree as of 2026-05-26:

```
a.out
dangling-link
default_17900404931741152552_0_1057015.profraw
fifo-file
file.txt
link
out.txt
output.txt
stderr.txt
stdout.txt
test-dir
test-file
test.txt
valid-link
~
```

These are byproducts of various test scripts that wrote into the cwd
instead of a tmp dir. Some are tracked (`a.out`, `link`, `~`?), some
are untracked but unignored. They make `git status` noisy and risk
accidental commits.

## Action

1. Inventory each file: is it produced by a script, a test, a manual
   experiment, or something already committed?
2. For each:
   - **Test-script byproducts** — fix the script to use a tmp dir, or
     extend `.gitignore`.
   - **Manual experiments** — `git rm` (or delete) and stop generating
     them in this directory.
   - **`.profraw`** — `*.profraw` should already be ignored; verify
     the glob covers nested dirs.
3. Add `mk clean` cases for any test-output that legitimately lands
   in the repo root.

## Why P5 (cheap, no decision needed)

A five-minute cleanup that prevents accidental commits and reduces
review noise. Independent of every other task.

## Out of scope

- Refactoring the test scripts that produce the cruft (do that lazily
  as each test is touched).
