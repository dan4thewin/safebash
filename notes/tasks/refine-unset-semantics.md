title: Refine unset semantics — scalar vs collection
status: ready
severity: correctness
gates: [30.7, 30.8, 30.9, 30.10, 30.11]
---

Converter: unset var (scalar) → var="" (30.7)
Compiler: unset var (scalar) → compile error (30.8)
Runtime: unset arr[N] removes element (30.9), unset map[key] removes key (30.10),
unset arr/map clears collection (30.11)
