#!/usr/bin/env bash
set -euo pipefail

build=$1
test "$(uname -s)" = Linux
python3 -E - "$build" <<'PY'
import collections
import pathlib
import re
import subprocess
import sys

build = pathlib.Path(sys.argv[1]).resolve()
root = build / "redirect-faults"
root.mkdir(exist_ok=True)
runner = build / "runtime-tests"

def run(name, injection=None, unwind=False):
    directory = root / name
    directory.mkdir(exist_ok=True)
    for leaf in ("output", "errors"):
        (directory / leaf).unlink(missing_ok=True)
    trace = directory / "trace"
    command = ["timeout", "10", "strace", "-qq",
               "-e", "trace=openat,fcntl,dup2,close,write", "-o", str(trace)]
    if injection:
        syscall, errno, ordinal = injection
        command += ["-e", f"inject={syscall}:error={errno}:when={ordinal}"]
    command += [str(runner), "--redirect-unwind-probe" if unwind else "--redirect-probe",
                str(directory)]
    with (directory / "stdout").open("wb") as out, (directory / "stderr").open("wb") as err:
        result = subprocess.run(command, stdout=out, stderr=err, check=False)
    return directory, result.returncode, trace.read_text()

directory, status, baseline = run("baseline")
assert status == 0, (status, (directory / "stderr").read_text())
assert (directory / "output").read_bytes() == b"body"
assert (directory / "errors").read_bytes() == b"__REDIRECT_INSTALLED__\n"
assert (directory / "stdout").read_bytes() == b"END\n"

counts = collections.Counter()
sites = {}
phase = "setup"
installed = False
for line in baseline.splitlines():
    call = re.match(r"(\w+)\(", line)
    if not call:
        continue
    syscall = call.group(1)
    counts[syscall] += 1
    if "__REDIRECT_READY__" in line:
        phase = "ready"
    elif "__REDIRECT_BODY__" in line:
        phase = "body"
    elif "__REDIRECT_FINISHED__" in line:
        phase = "finished"
    if syscall == "fcntl" and "F_DUPFD_CLOEXEC" in line:
        sites.setdefault("channels" if phase == "setup" else "save", counts[syscall])
    if phase == "ready":
        if syscall == "fcntl" and "F_GETFD" in line:
            sites.setdefault("inspect", counts[syscall])
        if syscall == "openat":
            key = "open-output" if "/output\"" in line else "open-errors"
            sites.setdefault(key, counts[syscall])
        if syscall == "dup2" and re.search(r", 1\)", line):
            sites.setdefault("install", counts[syscall])
            installed = True
        if syscall == "close" and installed:
            sites.setdefault("source-close", counts[syscall])
    if phase == "body":
        if syscall == "dup2":
            sites.setdefault("restore", counts[syscall])
        if syscall == "fcntl" and "F_SETFD" in line:
            sites.setdefault("restore-flags", counts[syscall])
    if phase == "finished" and syscall == "dup2":
        sites.setdefault("invoker-restore", counts[syscall])

cases = [
    ("channels", "fcntl", "EMFILE", "save descriptor", False, False),
    ("inspect", "fcntl", "EIO", "inspect redirect descriptor", True, False),
    ("save", "fcntl", "EMFILE", "save descriptor", True, False),
    ("open-output", "openat", "EACCES", "open redirect", True, False),
    ("open-errors", "openat", "EACCES", "open redirect", True, False),
    ("install", "dup2", "EIO", "install redirect descriptor", True, False),
    ("source-close", "close", "EIO", "close descriptor", True, False),
    ("restore", "dup2", "EIO", "restore redirected descriptors", True, True),
    ("restore-flags", "fcntl", "EIO", "restore redirected descriptors", True, True),
    ("invoker-restore", "dup2", "EIO", "restore invoker channels", False, True),
]
for key, syscall, errno, message, ended, body in cases:
    directory, status, trace = run(key, (syscall, errno, sites[key]))
    diagnostic = (directory / "stderr").read_text()
    assert status == 2, (key, status, diagnostic)
    assert re.search(rf"{syscall}\(.*{errno}.*INJECTED", trace), (key, trace)
    assert message in diagnostic, (key, diagnostic)
    assert ("__REDIRECT_END__" in diagnostic) == ended, (key, diagnostic)
    assert ("__REDIRECT_BODY__" in diagnostic) == body, (key, diagnostic)
    assert (directory / "stdout").read_bytes() == (b"END\n" if ended else b""), key
    if (directory / "errors").exists():
        assert (directory / "errors").read_bytes() in (b"", b"__REDIRECT_INSTALLED__\n"), key
    if body:
        assert (directory / "output").read_bytes() == b"body", key
    print(f"ok redirect fault {key}")

directory, status, trace = run("fatal-restore", ("dup2", "EIO", sites["restore"]), True)
diagnostic = (directory / "stderr").read_text()
assert status == 2 and "cleanup failed: restore redirected descriptors" in diagnostic
assert re.search(r"dup2\(.*EIO.*INJECTED", trace)
assert "__REDIRECT_END__" not in diagnostic and (directory / "stdout").read_bytes() == b""
assert (directory / "errors").read_bytes() == b"__REDIRECT_INSTALLED__\n"
print("ok redirect fault fatal-restore")

for key, syscall in (("open-output", "openat"), ("install", "dup2"), ("restore-flags", "fcntl")):
    directory, status, trace = run(key + "-EINTR", (syscall, "EINTR", sites[key]))
    assert status == 0, (key, (directory / "stderr").read_text())
    assert re.search(rf"{syscall}\(.*EINTR.*INJECTED", trace), key
    assert (directory / "output").read_bytes() == b"body", key
    assert (directory / "stdout").read_bytes() == b"END\n", key
    print(f"ok redirect retry {key}")
PY
