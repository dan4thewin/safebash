title: Lexical capture redirect proofs
date: 2026-11-25

# Lexical capture redirect proofs

DG reported SB4030 for an input redirect inside a capture after a successful
`cpath?` attest of the enclosing immutable binding. The intervening
readability guard was not responsible: the new proof checker required exact
source-body equality. That also matched the initial DD119 wording, so the
scope change went to DG rather than being silently broadened.

DG selected lexical capture inheritance. The AI implemented it on
`err-attempt-trial`, retaining the nontransitive rule and independent taint
checks. Main and DD136's adoption boundary are unchanged.

## Same binding, enclosing proof

The certificate retains its original source body and logical binding.
Redirect lookup checks that binding and walks outward only through capture
parents. It can consult an enclosing named body's own certificate, but
stops there rather than following the named function's caller.

`SourceOrigin` now distinguishes `Function` from `Capture` bodies. The shared
body-lowering entry and its three callers supply that distinction. Nearest-body
lookup otherwise keeps its existing meaning; regex and caught-read rules
are unchanged. There is no runtime property, new storage or ABI.

A capture's own proof remains unavailable to its parent or siblings.
Copies, derived bindings, function arguments/results and cells still do not
transfer certificates. Fresh-lifetime invalidation, all-reaching-path meets,
the simple-unary whitelist and redirect-site-only literal concatenation remain.
DD119, GUIDE, the outline and SB4030's advice now describe the revised scope.

## Focused evidence

Six new dual-mode specs raise `filetest.sh` from 109 to 115 checks per mode:

| Spec | Claim |
|---|---|
| `redirproof.36` | An enclosing named body's proof survives the readability guard and works in its capture, across repeated calls. |
| `redirproof.37` | A root proof reaches two capture levels; literal concatenation remains at the redirect. |
| `redirproof.38` | An outer capture's own proof reaches its nested capture. |
| `redirproof.39` | Caught-attest success admits the capture; failure follows its ordinary alternate path. |
| `redirproof.40` | Capture proofs do not escape to parents or siblings. |
| `redirproof.41` | Copying the value inside a permitted capture does not copy its proof. |

The frozen pre-repair compiler fails exactly `.36`-`.39` in both modes;
all other 111 assertions pass. The positive compiler passes all 115 in
both modes with ordinary taint checking.

Three independent source reversions run the whole file in both modes.
Every failure set matches its prediction, with no additional failures:

| Reversion | Failing `redirproof` suffixes, each mode |
|---|---|
| Require exact body equality again | 36, 37, 38, 39 |
| Accept every proof scope | 26, 40 |
| Ignore the certificate's binding identity | 19, 41 |

The scope reversion checks containment as a whole, not the named-body branch
in isolation; independent function-template checking remains in force.
All mutations are restored. Source, specs and the release compiler match
the saved positive hashes before and after the full gate.

## Preliminary failures and limits

The first native before-run also failed the existing timestamp cases
`ft.2`, `ft.3` and `ft.5`. The xtrace showed successful compilation but
unexpected false results. Absolute- and relative-path controls checked the
pinned timestamps before compilation, after compilation and after execution;
both returned the expected result. A traced whole-file run also passed those
cases, but its nested fault-injection tests encountered `PTRACE_TRACEME:
Operation not permitted`, so it is not an accepted control run.

The final untraced before pair, positive pair, all three reversion pairs
and full gate had no timestamp failures. Their original cause was not
established: no timestamp fix or external-reader explanation is claimed.
The traces and probe results are retained as evidence.

The reported platform script was not executed or changed. Its whole-script
compilation remains unestablished because platform commands are unavailable
here; the regressions exercise the reduced compiler shape independently.

## Full gate

```sh
NEW=12 FLIPS=0 RETIRED=0 TOTAL=13202 timeout 900 mk -p 1 gate
```

The gate passes at rows 7477879-7491080: 6717 native and 6485 C checks,
13189 passing plus 13 existing skips. The sweep took 338 seconds.
There are no failures, flips, retirements, ghosts or plan mismatches;
liveness is 246. An independent comparison with rows 7464689-7477878
preserves every one of the 13190 baseline statuses and finds exactly
`.36`-`.41` in both modes as the twelve first-ever IDs.

The included unit suites pass 241/27/20 tests. Runtime, sister-tool and both
local lockfile hashes remain unchanged, as do the two protected user files.
No coverage or unrelated implementation work was undertaken.
