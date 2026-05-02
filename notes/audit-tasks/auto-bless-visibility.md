title: Auto-bless undermines audit visibility
status: resolved
source: claude-opus-4.6, gemini-pro
severity: gap

DD#24 says bless is an audit construct — "auditors grep for `bless`."
But known_externals auto-blesses 26 env vars and 25 commands
invisibly. An auditor grepping for `bless` won't find auto-blessed
data in the source. `--emit-source` inserts explicit bless lines,
but the original source has invisible trust boundaries. The auto-bless
list in Rust code becomes a security-critical surface not visible in
the script under audit.

Resolution: The audit artifact is `--emit-source` output (sash-native form),
which already includes explicit `bless` lines for all auto-blessed items.
Auditors read that, not the bash-mode input. "Grep for bless" works as designed.
