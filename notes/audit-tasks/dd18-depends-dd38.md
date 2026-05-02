title: DD#18 depends on DD#38 — no function mocking path exists
status: resolved
source: claude-opus-4.6
severity: gap

DD#38 rejects `unset -f` and `declare -f`. DD#18 (`local fn`) is the
planned replacement but is only "designed." Until DD#18 is implemented,
there's no way to do function mocking/shadowing — old mechanism
rejected, new one doesn't exist.
