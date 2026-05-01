title: DD#23 — PATH Is Compile-Time Only
status: implemented

Scripts cannot modify PATH at runtime. External command lookup is resolved
at compile time. Eliminates PATH injection.
