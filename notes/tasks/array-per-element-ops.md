title: Array per-element operations
status: done
severity: feature
gates: [33.5, 33.6, 33.7]

${arr[@]/pat/rep}, ${arr[@]#prefix}, ${arr[@]%%suffix} — apply
operation to each element. 13 Brush test cases depend on this.
