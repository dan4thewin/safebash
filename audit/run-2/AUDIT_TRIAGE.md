# Adversarial Audit — Run 2: Taint/Bless/DD#46

**Date:** 2026-04-24
**Models:** gpt-4.1, claude-sonnet-4, claude-sonnet-4.5, gpt-5.2, gpt-5.3-codex
**Scope:** Taint model, bless runtime enforcement, DD#46 positional params
**Raw findings:** ~60 across 5 models → 12 deduplicated below

---

## Deduplicated Findings

### T1: Herestring/heredoc with tainted expansion marked clean
**Category:** PRODUCTION BUG
**Found by:** 5/5 (unanimous)
**Severity:** CRITICAL

`read y <<< "$tainted"` marks `y` clean because `handle_read()` checks
only for the *presence* of a herestring redirect, not whether the
content contains tainted expansions.

```bash
x=$(getenv UNTRUSTED)
read y <<< "$x"
echo "$y"   # y is clean — bypasses SB4020
```

**Also applies to heredocs:**
```bash
x=$(getenv SECRET)
read y <<EOF
$x
EOF
echo "$y"
```

---

### T2: Tautological bless test (bless without constraining variable)
**Category:** PRODUCTION BUG
**Found by:** 5/5 (unanimous)
**Severity:** CRITICAL

`bless x [[ 1 == 1 ]]` or `bless x [[ -n "literal" ]]` always
succeeds at runtime. The CST taint pass unconditionally blesses after
any test expression, and the runtime enforcement evaluates only the
expression — which doesn't reference `$x`.

```bash
x=$(getenv MALICIOUS)
bless x [[ 1 == 1 ]]
echo "$x"
```

---

### T3: Bless without valid test expression still blesses at CST level
**Category:** PRODUCTION BUG
**Found by:** 3/5 (gpt-5.2, gpt-5.3-codex, claude-sonnet-4.5)
**Severity:** HIGH

`bless x nope` — CST pass marks `x` blessed. Lowering can't find a
`TestExpr`, emits a regular command. No runtime enforcement, but
taint is "cleared" at compile time.

```bash
x=$(getenv SECRET)
bless x nope
echo "$x"
```

---

### T4: Arithmetic expansion not propagating taint through assignment
**Category:** PRODUCTION BUG
**Found by:** 3/5 (gpt-4.1, gpt-5.2, gpt-5.3-codex)
**Severity:** HIGH

`y=$((x + 1))` where `x` is tainted → `expr_is_tainted()` doesn't
handle `arithmetic_expansion` nodes, so `y` becomes clean.

```bash
x=$(getenv NUM)
y=$((x + 1))
echo "$y"    # y is clean — taint laundered via arithmetic
```

Note: *reading* tainted vars in `(( ))` and `$(( ))` contexts is now
caught by `check_arith_reads`. But the *assignment* path through
`expr_is_tainted` doesn't recognize arithmetic expansion as tainted.

---

### T5: Parameter expansion operand taint bypass
**Category:** PRODUCTION BUG
**Found by:** 2/5 (gpt-5.3-codex, claude-sonnet-4.5)
**Severity:** HIGH

`${safe:-$tainted}` — `check_expr_reads` and `expr_is_tainted` for
`expansion` nodes check the primary variable name but don't recurse
into operator operand trees (`:-`, `+`, `%`, `#` operands).

```bash
safe=""
x=$(getenv SECRET)
echo "${safe:-$x}"
```

---

### T6: Function global variable re-taint not tracked at call site
**Category:** TEST GAP
**Found by:** 1/5 (gpt-5.3-codex) — single-model, high value
**Severity:** HIGH

```bash
x="clean"
leak() { echo "$x"; }
x=$(getenv SECRET)
leak
```

`handle_function_def` analyzes the body at definition time (when `x`
is clean), snapshots/restores state. At call time, `x` is tainted,
but the function body was already validated. Inlining at SSA level
uses the current SSA version of `x`, which correctly contains the
tainted value — but no taint diagnostic is emitted.

---

### T7: Indirect expansion `${!var}` bypass
**Category:** HARDENING (already rejected by validator rule)
**Found by:** 4/5
**Severity:** LOW (mitigated)

`${!var}` is caught by `indirect_expansion.rs` validator rule (SB5001).
Not a bypass in practice, but taint.rs doesn't understand it either.

---

### T8: `declare -n` / nameref bypass
**Category:** HARDENING (already rejected by validator)
**Found by:** 3/5
**Severity:** LOW (mitigated)

`declare -n ref=tainted` — namerefs are not supported in the safe
subset. The validator rejects them. Not a bypass in practice.

---

### T9: DD#46 — shift then positional read
**Category:** TEST GAP
**Found by:** 3/5 (gpt-5.3-codex, gpt-4.1, claude-sonnet-4.5)
**Severity:** MEDIUM

```bash
f() { shift; echo "$1"; }
f one
```

`collect_max_positional` sees max=1, argc=1, no error. But after
`shift`, `$1` is out of range. Static analysis doesn't model `shift`.

---

### T10: Direct `$(getenv X)` in expression not caught
**Category:** TEST GAP
**Found by:** 1/5 (gpt-5.2)
**Severity:** MEDIUM

```bash
echo "$(getenv UNTRUSTED)"
```

`check_expr_reads` doesn't flag `command_substitution` as tainted
reads — only `simple_expansion` and `expansion`. The output of
`$(getenv X)` is consumed directly without an intermediate variable.

---

### T11: Case pattern taint not checked
**Category:** TEST GAP
**Found by:** 3/5 (gpt-4.1, claude-sonnet-4, claude-sonnet-4.5)
**Severity:** MEDIUM

Case *value* is checked, but pattern expressions in case items
containing `$tainted` are not.

---

### T12: Process substitution `<(cmd)` not taint-checked
**Category:** HARDENING
**Found by:** 2/5
**Severity:** LOW

Process substitution is likely rejected by the safe subset validator
already, but no explicit taint check exists.

---

## Summary by Action Priority

### fix-now (production bugs)
- **T1** — herestring/heredoc false clean
- **T2** — tautological bless
- **T3** — bless without test still blesses
- **T4** — arithmetic assignment taint laundering
- **T5** — parameter expansion operand taint

### strengthen (test gaps, non-trivial)
- **T6** — function global re-taint
- **T9** — shift + positional
- **T10** — direct $(getenv) in expression
- **T11** — case pattern taint

### defer (already mitigated or low priority)
- **T7** — indirect expansion (validator catches)
- **T8** — nameref (validator catches)
- **T12** — process substitution (validator likely catches)
