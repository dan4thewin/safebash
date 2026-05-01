# Design note: adversarial review (2026-03-28)

*Not an experiment — a future practice decision.*

DG tested adversarial LLM code review and test auditing on a more
mature codebase. The results were valuable. Two reasons to adopt
this practice in safebash once there is enough code to review:

1. **Credibility.** An AI-coded security tool will be dismissed
   out-of-hand unless it demonstrates rigorous independent review.
   Janet as the correctness oracle validates semantics, but the
   implementation itself (Rust code quality, unsafe patterns, logic
   errors in the validator) needs separate scrutiny.

2. **Blind spots.** The LLM that writes the code and the LLM that
   reviews it share training data and reasoning patterns, but a
   review pass with adversarial framing ("find bugs, find ways to
   bypass the safe subset") surfaces issues that a constructive pass
   misses. DG observed this empirically.

**When to introduce it:** after the validator framework exists
(experiment 011) and there are real accept/reject decisions to
audit. Before that, there's nothing meaningful to attack. The
right trigger is phase 3 (IR generation) — by then the compiler
has enough moving parts that an adversarial pass can find real
issues rather than style nits.

**What it is not:** a replacement for Janet. Janet validates that the
compiler's output is semantically correct. Adversarial review
validates that the compiler's implementation is sound. Different
failure modes, both needed.

---

