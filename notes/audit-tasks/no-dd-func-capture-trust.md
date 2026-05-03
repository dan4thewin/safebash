title: No decision on user-function capture trust boundary
status: resolved
source: gpt-5.4
severity: gap

SUBSET.md says user-defined functions are trusted and $(user_func)
capture is clean. This is a critical trust boundary — external command
capture is tainted, internal function capture is clean — but no single
DD explicitly states the declassification rule and proof obligations.
It's a derived property scattered across DD#27, DD#50, DD#51.
