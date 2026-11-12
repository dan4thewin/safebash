title: Array and map key type enforcement
status: done
severity: correctness
gates: [33.1, 33.2, 33.3, 33.4, 41.1, 41.2]

DD#33/DD#41: indexed arrays use integer keys, maps use string keys,
string key on array is compile error, integer key on map is compile error.
array arr=(...) and map m=(...) accepted as declaration syntax.
