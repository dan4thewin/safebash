title: DD#27 — interactions with DD#38 and DD#50 not documented
status: resolved
source: gpt-5.4
severity: accuracy

DD#27 (trampoline) doesn't document: how `my_func &` (DD#38) forks
to trampoline blocks in child, interaction with pipe-free capture
(task), or how the single-function model affects DD#50 free-read
analysis. These are derived properties scattered across docs.
