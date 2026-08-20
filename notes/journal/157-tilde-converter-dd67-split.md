# 157 — tilde converter half; dd67 §3 split out

DG: "finish the p3 partials." The two P3 partials were `tilde-vs-dd53` and
`dd67-bash-sugar-sweep`. Both had their *compiler* halves done already; the
remainders were convert-tier. tilde's remainder was a small, self-contained
converter rewrite — so I finished it. dd67's remainder (§3 canonicalize
mode) is a substantial converter *feature*, now unblocked (dd68 done) but
Tier-6-sized — so I split it to its own task and marked dd67's compiler
scope done.

## tilde converter half

The compiler rejects an unquoted leading `~`/`~/path` with SB4036 (only
`w == "~" || w.starts_with("~/")` — `~root` is a literal). The converter's
job is to make bash-with-`~` translate to compiling sash.

tree-sitter-bash parses **every** tilde form as a plain `word` node — there
is no dedicated tilde node. So `convert_tilde_word` hooks a new `"word"` arm
in `walk_node` and inspects the text:

- `~` exactly, or `~/…` with no further `~` in the tail → rewrite to
  `$home` / `$home/<tail>` and hoist `home=$(getenv HOME); bless home [[
  $home =~ ^/[\w./\-]+$ ]]` (the exact `$HOME` env-var hoist, reusing
  `known_externals::lookup_env_var("HOME")`). A script that mixes `$HOME`
  and `~` gets the same `home` var — consistent.
- `~user` / `~user/…` → **MANUAL** note, leave unchanged. sash has no passwd
  lookup, and the compiler treats `~root` as a *literal* (no SB4036), so a
  silent pass-through would differ from bash — the note forces review.
- multi-tilde word `~/bin:~/lib` → **MANUAL**, leave unchanged. A partial
  rewrite of only the leading `~` would silently drop the second one; the
  whole word starts with `~/` so SB4036 fires → review.
- command-position `~/bin/prog` (parent kind `command_name`) → **MANUAL**,
  leave unchanged. `$home/bin/prog` is a dynamic command path that DD#23
  rejects, so rewriting it would just move the error.

### Why quoting needs no special-casing

`'~'` parses as `raw_string`, `"~"` as `string`, `a~b`/`foo~` as `word`s
whose text doesn't start with `~`. Only a genuine unquoted leading tilde is
a bare `word` starting with `~`, so the node-kind + `starts_with('~')` guard
is exactly right. Arithmetic `~` (bitwise-not) is a `unary_expression`, never
a word, so it can't misfire.

### `~/$x` composes for free

`~/$x` parses as `concatenation(word "~/", simple_expansion $x)`. The `~/`
arrives at the word arm on its own → `$home/`; the recursion converts `$x`
separately → `$home/$x`. No special handling needed.

### Verified

End-to-end (convert → `sash` compile → run) is byte-identical to bash:
`echo ~` → `/home/x`, `echo ~/foo` → `/home/x/foo`, `x=~/p; echo $x` →
`/home/x/p` (with `HOME=/home/x`). spec/convert.sh 57.47-57.55 (native-only),
Rust convert_tests.rs tilde_* (4). All 18 converter Rust tests pass.

### The 53.x collision (caught before commit)

I first numbered the converter tests 53.1-53.9 (DD#53 = tilde). But
spec.db test_ids are **global** — `chk` records `$SASH_MODE:$id`, and
`spec/taint.sh` already owns `53.1`-`53.10` (the compiler-side tilde tests).
convert.sh sorts before taint.sh, so in a native sweep convert's `native:53.1`
was overwritten by taint's — a real collision (two distinct tests sharing an
id), masked only because both passed. Renumbered into convert.sh's DD#57
namespace at 57.47-57.55 (global max across files was 57.46; convert.sh 1-41,
compiler.sh 42-46). Lesson: `chk` ids are a flat global namespace across all
spec files — check `grep -rn "chk N\." spec/*.sh` before picking a block.

## dd67 §3 split

dd67's §1 (`name()`/`function`), §2 (backticks), §4B (`[` literal), and the
§4 keep/drop table are the whole *compiler* scope — done. §3 (the sash-convert
canonicalize / native-only mode: `then…fi`/`do…done` → `{ }`, `name()` →
`fn`, `` `…` `` → `$()`) needed dd68's brace forms to exist. dd68 is now
done, so §3 is unblocked — but it's a distinct Tier-6 converter feature, so
it moves to its own task `sash-convert-canonicalize-mode` (ready). dd67 is
marked done rather than left lingering as "partial."

## Files

- `sash-convert/src/convert.rs` — `convert_tilde_word` + `"word"` arm.
- `sash-convert/tests/convert_tests.rs` — tilde_bare_home, tilde_home_path,
  tilde_user_is_manual, tilde_quoted_is_literal.
- `spec/convert.sh` — 57.47-57.55 (plan 61 → 70).
- `notes/tasks/tilde-vs-dd53.md` → done; `dd67-bash-sugar-sweep.md` → done;
  `sash-convert-canonicalize-mode.md` (new, ready).
