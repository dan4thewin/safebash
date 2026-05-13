title: String runtime — pointer+length, null-terminated, O(1) length
status: done
severity: correctness
gates: [12.1, 12.2, 12.3]
---

Verify string representation meets DD#12 spec:
pointer+length pair, null-terminated for C interop, O(1) length queries.
These may already work — tests may need sash -c compilation fixes.
