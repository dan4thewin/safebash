#!/bin/bash
# spec/57.64.sh — sash SHALL accept trivial bash indistinguishable from sash-native
set -eu
. common.sh
setup 57.64
plan=2
echo "1..$plan" >&3

# x=hello is valid in both bash and sash-native
run_sash 'x=hello
echo $x'
(( rc == 0 )); chk 57.64a
[[ $out == "hello" ]]; chk 57.64b

tally
