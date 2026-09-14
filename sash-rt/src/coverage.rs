//! Coverage dump for the `no_std` runtime — feature `coverage` only.
//!
//! `-C instrument-coverage` compiles and maps against `sash-rt` perfectly
//! well: the counters and the `__llvm_prf_*` sections land in every compiled
//! binary. What never happens is the **write**. `__llvm_profile_write_file()`
//! is normally registered as an `atexit` handler by compiler-rt's profiling
//! runtime, and that function wants libc — path munging, `FILE*`, `flock`,
//! `getenv`, `snprintf`. A `no_std` / no-libc / `panic=abort` static ELF has
//! none of it, so the process exits with live counters and leaves nothing
//! behind.
//!
//! The way out is that `__llvm_profile_write_buffer()` is a *different object*
//! from `__llvm_profile_write_file()`, and it does not want libc. Taking the
//! symbol closure of the buffer path out of `libclang_rt.profile-x86_64.a`:
//!
//! ```text
//! InstrProfilingBuffer.c.o        InstrProfilingWriter.c.o
//! InstrProfilingPlatformLinux.c.o InstrProfilingInternal.c.o
//! InstrProfilingVersionVar.c.o    InstrProfilingNameVar.c.o
//! InstrProfiling.c.o                                    — 52 KB total
//! ```
//!
//! `ld -r` over those seven leaves exactly three strong undefined symbols:
//! `__ehdr_start`, which the linker supplies in any static link, and `memcpy`
//! / `memset`, which this crate already defines. The `__start___llvm_prf_*` /
//! `__stop___llvm_prf_*` section bounds are weak and linker-provided.
//!
//! So compiler-rt does the serialising and we supply only the destination.
//! Nothing here knows the `.profraw` layout, which is the point: an LLVM bump
//! cannot silently corrupt the output, because we never encode it.
//!
//! `mk rt-cov` does the linking; see the mkfile.

// Provided by the compiler-rt objects `mk rt-cov` folds into the archive.
extern "C" {
    fn __llvm_profile_get_size_for_buffer() -> u64;
    fn __llvm_profile_write_buffer(buf: *mut u8) -> i32;
}

/// `<dir>/<pid>-<nanos>.profraw`. Long enough for any plausible directory;
/// a longer one is declined rather than truncated.
const PATH_MAX: usize = 512;

/// Write a `.profraw` for this process if `SASH_RT_PROFILE_DIR` names one.
///
/// Silent on every failure. This runs on the way out of a program the user
/// asked to run, not a program they asked to measure — a coverage harness
/// that writes to stderr would corrupt the output of every spec test that
/// compares against bash.
pub(crate) fn dump() {
    let Some(dir) = env_value(b"SASH_RT_PROFILE_DIR=") else {
        return;
    };
    let size = unsafe { __llvm_profile_get_size_for_buffer() } as usize;
    if size == 0 {
        return;
    }

    // mmap, not `sash_alloc`. This runs at exit, after arbitrary program
    // activity, and the heap is the one thing a runtime bug is most likely to
    // have left unusable — taking the profile through it would lose exactly
    // the runs worth measuring.
    let Some(buf) = map_anon(size) else {
        return;
    };
    if unsafe { __llvm_profile_write_buffer(buf) } != 0 {
        return;
    }

    let mut path = [0u8; PATH_MAX];
    let Some(len) = build_path(&mut path, dir) else {
        return;
    };
    write_file(&path[..len + 1], buf, size);
}

/// `<dir>/<pid>-<nanos>.profraw`, NUL-terminated. Returns the length before
/// the NUL, or `None` if it would not fit.
///
/// The pid alone is not unique enough: a full spec sweep runs thousands of
/// short-lived binaries and the kernel recycles pids, so two runs would
/// collide and one would be silently lost. The nanosecond stamp disambiguates.
fn build_path(out: &mut [u8; PATH_MAX], dir: &[u8]) -> Option<usize> {
    let mut n = 0;
    let mut push = |bytes: &[u8], n: &mut usize| -> bool {
        if *n + bytes.len() + 1 > PATH_MAX {
            return false;
        }
        out[*n..*n + bytes.len()].copy_from_slice(bytes);
        *n += bytes.len();
        true
    };
    if !push(dir, &mut n) {
        return None;
    }
    if !push(b"/", &mut n) {
        return None;
    }

    let mut num = [0u8; 24];
    let pid = rustix::process::getpid().as_raw_nonzero().get() as u64;
    let d = render_u64(&mut num, pid);
    if !push(d, &mut n) {
        return None;
    }
    if !push(b"-", &mut n) {
        return None;
    }

    let t = rustix::time::clock_gettime(rustix::time::ClockId::Monotonic);
    let nanos = (t.tv_sec as u64).wrapping_mul(1_000_000_000) + t.tv_nsec as u64;
    let mut num2 = [0u8; 24];
    let d = render_u64(&mut num2, nanos);
    if !push(d, &mut n) {
        return None;
    }
    if !push(b".profraw", &mut n) {
        return None;
    }
    out[n] = 0;
    Some(n)
}

/// Decimal, no allocator and no formatting machinery. Returns the filled tail
/// of `buf`.
fn render_u64(buf: &mut [u8; 24], mut v: u64) -> &[u8] {
    let mut i = buf.len();
    if v == 0 {
        i -= 1;
        buf[i] = b'0';
        return &buf[i..];
    }
    while v > 0 {
        i -= 1;
        buf[i] = b'0' + (v % 10) as u8;
        v /= 10;
    }
    &buf[i..]
}

fn map_anon(size: usize) -> Option<*mut u8> {
    let p = unsafe {
        rustix::mm::mmap_anonymous(
            core::ptr::null_mut(),
            size,
            rustix::mm::ProtFlags::READ.union(rustix::mm::ProtFlags::WRITE),
            rustix::mm::MapFlags::PRIVATE,
        )
    };
    match p {
        Ok(p) => Some(p as *mut u8),
        Err(_) => None,
    }
}

fn write_file(path_with_nul: &[u8], buf: *mut u8, size: usize) {
    let Ok(path) = core::ffi::CStr::from_bytes_with_nul(path_with_nul) else {
        return;
    };
    let flags = rustix::fs::OFlags::WRONLY
        .union(rustix::fs::OFlags::CREATE)
        .union(rustix::fs::OFlags::TRUNC);
    let Ok(fd) = rustix::fs::open(path, flags, rustix::fs::Mode::from_raw_mode(0o644)) else {
        return;
    };
    let mut written = 0usize;
    while written < size {
        let slice = unsafe { core::slice::from_raw_parts(buf.add(written), size - written) };
        match rustix::io::write(&fd, slice) {
            Ok(0) | Err(_) => break,
            Ok(n) => written += n,
        }
    }
}

/// `prefix` includes the `=`. Returns the value, borrowed from `environ`.
fn env_value(prefix: &[u8]) -> Option<&'static [u8]> {
    let envp = unsafe { crate::sash_envp };
    if envp.is_null() {
        return None;
    }
    let mut i = 0;
    loop {
        let entry = unsafe { *envp.add(i) };
        if entry.is_null() {
            return None;
        }
        let len = unsafe { crate::strlen(entry) };
        let slice = unsafe { core::slice::from_raw_parts(entry, len) };
        if len > prefix.len() && &slice[..prefix.len()] == prefix {
            return Some(&slice[prefix.len()..]);
        }
        i += 1;
    }
}
