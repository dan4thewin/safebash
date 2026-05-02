title: DD#26 — "bump allocator" is factually wrong
status: resolved
source: claude-opus-4.6, gpt-5.4
severity: accuracy

Resolved: DD#26 corrected. heap_4 is a first-fit free-list allocator
with coalescing, not a bump allocator. The runtime source already had
the correct comment ("first-fit + coalesce"). GUIDE.md had no reference
to fix.
