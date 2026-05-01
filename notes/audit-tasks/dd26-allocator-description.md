title: DD#26 — "bump allocator" is factually wrong
status: ready
source: claude-opus-4.6, gpt-5.4
severity: accuracy

DD#26 and GUIDE.md both call heap_4 a "bump allocator." FreeRTOS
heap_4 is a first-fit free-list allocator with block coalescing —
it supports free(), which bump allocators do not. Fix the description
in the decision note and GUIDE.md.
