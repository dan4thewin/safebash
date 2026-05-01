title: DD#24 — mechanism change (propagation → classification) not captured
status: ready
source: claude-opus-4.6, gpt-5.4
severity: accuracy

DD#24 note references DD#43/44 amendments but doesn't capture the
fundamental mechanism change from DD#24 amendment 2: taint changed from
flow-sensitive propagation to per-definition classification with
use-site rejection. The note's framing is correct for the final state
but the mechanism shift isn't documented. GUIDE.md still describes
"flow-sensitive taint tracking" which invites the wrong mental model.
