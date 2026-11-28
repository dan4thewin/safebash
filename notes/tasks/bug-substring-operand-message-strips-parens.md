title: "bug: SB4030's substring message quotes the operand with its parens stripped"
status: done
priority: P3
kind: bug
depends: []
gates:
  - "`s=abc; echo \"${s:0:$(( 1 ))}\"` quotes the operand as written (`$(( 1 ))`), not \"`$ 1 `\""
  - "`${s:(-2)}` still compiles and prints `bc`"

## Resolution, 2026-11-28

The parser and type checker share field validation. It normalizes
parentheses for the existing integer check but retains the original
spelling of an invalid field. Valid operands keep their prior normalized
representation and behavior; no parenthesis-grammar change is introduced.
Strengthened `sub.9` pins `(1+1)`, and new `diagmsg.1` pins `$(( 1 ))`
after a valid parenthesized offset. Independent source-retention and
per-field reversions fail those assertions; both modes pass the full gate.
[Journal 684](../journal/684-unattended-diagnostics-and-c-literals.md)
records the evidence.

## Original report

Found 2026-10-31 while porting F10 (journal 552). It reproduces on the pre-F10
binary too.

```sh
s=abc
echo "${s:0:$(( 1 ))}"
```

The error is right, but the message quotes a different operand:

```
2:6: error [SB4030] substring offset/length must be a decimal integer — `$ 1 ` is not. …
```

**Cause.** `try_substring_op` (parse/expr.rs:1480) runs
`sub_content.replace(['(', ')'], "")` over the whole offset/length text. The
replace exists for the `${s:(-2)}` wrapper, but it also removes the parens of
`$(( 1 ))` and of any other operand. So `check_substring_operand`
(type_check.rs:1172) is handed `0:$ 1 ` and quotes that.

**Fix.** Strip only one wrapping `( … )` around each field, or keep the
original text for the message. The stripping also changes what is checked,
not only what is printed: `${s:(1)+(2)}` is checked as `1+2`. Measure what
each spelling compiles to before choosing.
