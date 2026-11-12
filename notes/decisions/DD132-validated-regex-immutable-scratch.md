title: DD#132 - Validated regex values, immutable scratch and boolean results
status: provisional

1. **Trial only:** evaluate with DD131 on a separate branch before DG's
   adoption decision. Freshness, immutability, cleanliness and regex validity
   are independent properties.
2. `r?` tests the complete candidate using the consumers' regex dialect.
   Invalid syntax, including literal candidates, gives quiet false;
   resource faults remain fatal.
3. Every runtime pattern must be immutable and successfully validated on
   the reaching path. Proof follows the same value/instance; composition
   needs a new check. Literal patterns retain static validation.
4. Regex-pattern reads cannot use `:!`; disabling taint checks supplies no
   validity proof. Subject and replacement reads retain ordinary rules.
5. A caught attest of immutable scratch permits ordinary reads only in its
   success arm. Elsewhere only trusted data reads are allowed, not pattern
   use; the unchanged failed instance may remain unresolved.
6. That permission never escapes the caught construct, even after a
   noncontinuing failure arm. Standalone attest permits subsequent uses.
   Ordinary bindings retain their resolution obligations.
7. `match`, `matchall` and substitutions return matched/no-match booleans.
   Substitution also returns text: no match preserves the subject;
   unchanged bytes can match.
8. Value-bearing regex captures can consume the boolean with their text;
   `-` may discard that boolean. Value substitution sugar projects text;
   mutation remains resultless. Every pattern still requires proof.
9. Keep selector operational statuses, I/O errors and fatal faults distinct
   from invalid-candidate and no-match answers.

---

DG provisionally adopted this model on 2026-11-09 (journal 604), together
with the revisions to DD131 and the existing DDs:

> let's provisionally adopt this along with the revisions to existing DD -
> in the implementation task let's keep track of which DD were modified
> should any part prove untenable

`q-glob-semantics` owns the **integrated separate-branch trial** of
selectors, immutable scratch and these regex rules. Its DD revision
inventory records the pre-revision commit, every affected DD and the
contract to revisit. `q-dynamic-regcomp-levers` retains the historical
question/evidence and points to that implementation owner. Nothing here
authorizes starting implementation or landing a prototype on main.

## Three independent properties

- **Freshness:** DD131 gives each execution of a scratch construction
  site a new instance, without the previous value or proof.
- **Immutability:** the constructed instance cannot be written; an
  otherwise legal attest may validate its unchanged value.
- **Regex validity:** a runtime pattern must have successfully passed
  `r?`, with that proof available at the use of the same immutable value.

A clean string need not be a valid regex. A const need not be clean or
valid. Neither `:!` nor a taint-disable switch establishes regex validity.
Testing a mutable value does not freeze its binding or make that binding
eligible as a runtime pattern; construct an immutable instance first.
Scratch is the repeatable construction mechanism, not the only possible
immutable value: an ordinary const can also be validated and used.
No new source-level regex type, handle syntax, runtime cache or ABI is
selected by this policy.

## `r?`: test the complete candidate

`r?` joins DD93's unary content-shape tests:

```sash
if [[ r? $candidate ]] { ... }   # ordinary read: candidate must be clean
attest _re r?
if attest _re r? { ... } else { ... }
attest p in _patterns r?
```

It is true exactly when the whole candidate is an accepted sash regex,
using the same engine and dialect rules as consumers. Invalid syntax
returns **quiet false**, not a diagnostic plus a success-shaped value.
An invalid literal supplied as a candidate is data being tested: it
does not receive SB5023 merely because `r?` was asked about it.
Literal patterns used as regexes still receive static validation.

Quiet describes the predicate, not a standalone attest's ordinary
failure diagnostic. Cleanliness alone must not erase a needed validity
check; a clean runtime string can still be an invalid pattern.

Resource exhaustion and other fatal faults remain fatal under DD130.
Do not collapse an allocation failure into "invalid regex". The
predicate must not be a second approximate regex grammar. Empty/NUL,
escape and dialect-limit cases must agree with the accepted pattern
contract; this does not change DD63's separate subject-interior-NUL
limitation or claim that a syntactically valid pattern is useful.

An ordinary test establishes a validity fact on its successful path,
not taint clearance. Attest also supplies its normal explicit
declassification within the permitted region. Existing predicate
restrictions, adjacency and body/taint ownership remain. A full
predicate grants regex proof only where its success proves that `r?`
succeeded; merely mentioning `r?` in a disjunction is not enough.

## Runtime regex operands require proof

Every runtime regex consumer requires an immutable pattern with the
successful check established before use: `match`, `matchall`, `sub`,
`gsub`, `[[ =~ ]]`, regex dispatch arms, substitution sugar and DD131's
`rglob`. This is a compiler-enforced prerequisite, not optional preflight
or a rule satisfied by an earlier test somewhere in the source.

Whole-scalar quoting preserves the value. Concatenation, case conversion,
substitution and other byte-changing composition produce a new candidate;
construct and validate the **finished** pattern. The same applies inside
interpolating sugar. A proof for one piece does not certify the assembled
regex. An immutable collection's otherwise-legal successful attest-all
can certify its present elements; iteration may preserve that fact.
A missing-index default is not an element that was checked.

Proof follows actual values and instances, not permanently trusted source
names. Reconstruction cannot inherit it. Calls must meet a checked
pattern requirement or validate locally; ordinary clean arguments do
not imply regex validity. Collection identity/immutability must survive
A-E forwarding. Globals, captures, children and EXIT/ERR must not obtain
proof merely by naming a binding checked in another execution context.
Preserve the existing independent-body and handler-entry contracts.

**No `:!`-bearing read in a regex-pattern operand**, including combined
trust/disposition forms and indirect spellings through sugar. This
restriction applies even inside a successful attest arm and with taint
checking disabled. It is about the operand's role, not the literal bytes
`:!` in a pattern. Trusted subject or replacement reads remain subject
to their ordinary rules; this is not a blanket ban on `:!` anywhere in a
regex invocation.

A trusted copy such as `_copy=${_re:!}` is ordinary data, not a validated
regex. It may become a regex after its own successful check. Copying or
calling a helper does not itself export the original instance's
branch-limited permission past the caught-attest boundary below.

## Caught attests on immutable scratch

DG proposed the rule that replaces the earlier abandonment idea:

> for the final case of a caught attest on an immutable scratch, we could
> allow use of the scratch with match/sub/gsub/... only in the true branch,
> and only allow :! use outside the true branch, but no :! with match et al.

For a caught attest on immutable scratch, ordinary reads are permitted
only in its **attest-success arm**. Regex uses additionally require the
successful `r?` proof, and cannot use a trust modifier. In the failure
arm and after the caught construct, only explicit trusted reads as
ordinary data are permitted. Eligible attest-predicate reads retain
their normal rules; this grants no late or inherited-input attest.

| Context | Ordinary data read | Explicit trusted data read | Regex-pattern use |
|---|---|---|---|
| Attest-success arm | Allowed | Allowed | With `r?` proof, without `:!` |
| Failure arm or after the caught construct | Rejected | Allowed | Rejected |

These are the normal checked-program permissions. Regex proof and the
pattern-trust prohibition are independent of taint-disable switches.
The successful arm is the arm where the **attest** succeeded: under
`if ! attest ...`, it is the `else` arm. Logical caught forms must respect
their corresponding success/failure regions; they cannot export a
synthetic successful status or widen the proof's scope.

The boundary is deliberately lexical for the caught form. Even an
`else { continue }`, `return`, `exit` or `fail` does not export permission
to code after the `if`. A standalone, implicitly checked attest is
different: only success continues, so subsequent legal uses are allowed.

```sash
while readline _re {
    if attest _re r? {
        if match "$text" "$_re" {
            echo matched
        }
    } else {
        echo "invalid regex: ${_re:!}" >&2
        continue
    }
}
```

The failed candidate remains intact and immutable. No discard operation,
automatic abandonment, clean replacement or declassification occurs.
The scratch-specific relaxation of DD121/DD128 is that an unresolved
failed instance **may fall through**, or a caught form may omit `else`,
provided subsequent uses obey the restricted permissions. Ordinary
bindings keep R2's resolution requirement. Ordinary consts do not acquire
this exception just because they are immutable.

DD121 already exempts paths that do not fall through, including `continue`
and `return`. The AI's earlier discussion overstated that obstacle.
Permitting `continue` is not the new permission; unresolved fall-through
and success-arm-only reads are. Loop state, globals and EXIT/ERR still
obey their independent checks. Signal protection must not falsely clean
the failure arm or extend dispatch deferral for an unresolved instance's
entire lifetime.

## Regex operations have logical results

Once patterns are proved valid, malformed regex data is not a third
result from a later consumer. The regex operation's answer is logical:

| Operation | True | False | Data result |
|---|---|---|---|
| `match` | A match occurred | No match | Existing whole-match capture or out-array shape, when requested |
| `matchall` | At least one match | No match | Existing mandatory out-array extraction shape |
| `sub` | A match occurred | No match | First replacement result, or the original subject |
| `gsub` | At least one match | No match | Global replacement result, or the original subject |

True means **matched**, not "output bytes changed"; an identity
replacement can still be true. Substitution's original-subject result on
no match is valid output, not recovery from invalid regex syntax.
DD131 controls ordinary accumulation versus immutable scratch out-arrays.
The verdict describes this operation, never old entries in a destination.

`sub`/`gsub` gain a real boolean **alongside** their text, not instead of
it. Existing value-producing regex forms can bind their text and consume
the boolean through a capture-condition:

```sash
if _out=$(sub "$text" "$_re" replacement) {
    echo "matched: $_out"
} else {
    echo "no match: $_out"
}
```

Both branches have a valid result. This extends DD88 for real regex
value-plus-boolean producers, not arbitrary assignments, integer values,
user-function bodies or multi-statement captures. `matchall` still has
no scalar value form. Assignment itself remains resultless.

DD70 requires a boolean to be consumed or explicitly discarded:

```sash
_out=$(- sub "$text" "$_re" replacement)
```

The prefix discards the boolean while the capture consumes the text.
It is no longer an inert prefix on a status-less intrinsic. Other
status-less intrinsic captures retain their rejection; no new bare
statement-position meaning is granted to them. Pointless-work checks,
no-reach, operand evaluation and fatal faults remain.

Substitution **value sugar** is explicitly a text-only projection of
the operation, discarding its boolean by definition. Mutation sugar
remains a resultless write and is forbidden on immutable scratch.
Chains retain their order and project each step's text; no new combined
chain verdict is invented. Every pattern in every sugar form still owes
the same proof. This projection is an explicit provisional change to
DD63/DD65/DD70, not an implicit exception made by a backend.

`glob` and `rglob` retain operational statuses: successful enumeration
is success even with no matches. `rglob` requires validated patterns but
can still encounter directory failures. Fatal faults and ordinary I/O or
redirect failures do not become regex no-match or predicate-invalid
answers.

## Evaluation and implementation boundary

The trial must measure the useful cases and the costs: scalar/read/for
construction, captures and immutable collections, caught failure logging,
no-else fall-through, skipped sites and zero-trip loops, helper/slot
requirements, builders versus snapshots, text-only substitution callers,
dynamic sugar and converter migration. Preserve meaningful zeros without
inventing a regex proof for them.

Use the real CLI and both backends, before/reversion controls and measured
gate predictions. Pattern validation must agree with actual consumers;
syntax validity alone does not repair the separately owned regex-anchor
bug. Ordinary const validation remains its existing prerequisite.

This is policy/backlog recording only. The compiler does not yet implement
`r?`, mandatory regex proofs, immutable scratch or substitution booleans.
No diagnostic number, lowering/proof-pass architecture, compiled-program
retention scheme or final scratch declaration grammar is selected.
