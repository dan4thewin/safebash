title: DD#30 — Strict Type Separation
status: implemented

No implicit conversion between int and string. Conversion happens
explicitly at boundaries: `$(( ))` for string→int, `"$var"` for
int→string (at output).
