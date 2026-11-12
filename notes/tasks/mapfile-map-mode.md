title: Implement mapfile -m for KEY=VALUE → map
status: done
severity: feature
gates: [22.8, 22.9, 22.10, 22.11]

mapfile -m reads KEY=VALUE lines into a map. Skips blanks and # comments.
Splits on first = only. Result is tainted (external input).
