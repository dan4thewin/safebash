//! Private directory selection; only completed batches reach source collections.

use core::{mem, ptr, slice};
use rustix::fs::{open, Mode, OFlags, RawDir};

use crate::{
    alloc_cstr, glob_pattern, sash_abort, sash_alloc, sash_arith_overflow, sash_emsg, sash_free_c,
    sash_getcwd, sash_table_set, set_last_errno, SashTable, TableBatch,
};

struct Bytes {
    ptr: *mut u8,
    len: usize,
}

impl Bytes {
    fn new(len: usize) -> Self {
        len.checked_add(1)
            .filter(|&n| n <= isize::MAX as usize)
            .unwrap_or_else(|| sash_arith_overflow());
        Self {
            ptr: unsafe { alloc_cstr(len) },
            len,
        }
    }

    fn as_slice(&self) -> &[u8] {
        unsafe { slice::from_raw_parts(self.ptr, self.len) }
    }

    fn as_mut_slice(&mut self) -> &mut [u8] {
        unsafe { slice::from_raw_parts_mut(self.ptr, self.len) }
    }
}

impl Drop for Bytes {
    fn drop(&mut self) {
        unsafe { sash_free_c(self.ptr) }
    }
}

#[derive(Clone, Copy)]
struct Path {
    ptr: *mut u8,
    len: usize,
}

impl Path {
    fn bytes(&self) -> &[u8] {
        unsafe { slice::from_raw_parts(self.ptr, self.len) }
    }
}

struct Paths {
    ptr: *mut Path,
    len: usize,
    capacity: usize,
}

impl Paths {
    fn new() -> Self {
        Self {
            ptr: ptr::NonNull::<Path>::dangling().as_ptr(),
            len: 0,
            capacity: 0,
        }
    }

    fn push(&mut self, prefix: &[u8], name: &[u8]) {
        if self.len == self.capacity {
            self.grow();
        }
        let separator = usize::from(prefix != b"/");
        let len = prefix
            .len()
            .checked_add(separator)
            .and_then(|n| n.checked_add(name.len()))
            .unwrap_or_else(|| sash_arith_overflow());
        let mut bytes = Bytes::new(len);
        let out = bytes.as_mut_slice();
        out[..prefix.len()].copy_from_slice(prefix);
        if separator != 0 {
            out[prefix.len()] = b'/';
        }
        out[prefix.len() + separator..].copy_from_slice(name);
        unsafe {
            self.ptr.add(self.len).write(Path {
                ptr: bytes.ptr,
                len,
            });
        }
        mem::forget(bytes);
        self.len += 1;
    }

    fn grow(&mut self) {
        let capacity = self
            .capacity
            .max(8)
            .checked_mul(2)
            .unwrap_or_else(|| sash_arith_overflow());
        let size = capacity
            .checked_mul(mem::size_of::<Path>())
            .filter(|&n| n <= isize::MAX as usize)
            .unwrap_or_else(|| sash_arith_overflow());
        let next = unsafe { sash_alloc(size).cast::<Path>() };
        unsafe {
            ptr::copy_nonoverlapping(self.ptr, next, self.len);
            if self.capacity != 0 {
                sash_free_c(self.ptr.cast());
            }
        }
        self.ptr = next;
        self.capacity = capacity;
    }

    fn sort_from(&mut self, start: usize) {
        let paths = unsafe { slice::from_raw_parts_mut(self.ptr, self.len) };
        paths[start..].sort_unstable_by(|a, b| a.bytes().cmp(b.bytes()));
    }

    unsafe fn publish(&mut self, target: *mut SashTable) {
        for i in 0..self.len {
            let path = &*self.ptr.add(i);
            let key = i as i64;
            sash_table_set(
                target,
                (&key as *const i64).cast(),
                8,
                path.ptr,
                path.len as i64,
            );
        }
        // The table now retains the strings; only the temporary vector is freed.
        self.len = 0;
    }
}

impl Drop for Paths {
    fn drop(&mut self) {
        for i in 0..self.len {
            unsafe { sash_free_c((*self.ptr.add(i)).ptr) }
        }
        if self.capacity != 0 {
            unsafe { sash_free_c(self.ptr.cast()) }
        }
    }
}

enum Failure<'a> {
    Pattern(&'a [u8], &'static str),
    System(&'static [u8], &'a [u8], rustix::io::Errno),
}

impl Failure<'_> {
    fn record(&self, checked: bool) {
        let (operation, path, errno) = match self {
            Self::System(operation, path, errno) => (*operation, *path, *errno),
            Self::Pattern(pattern, reason) => {
                (reason.as_bytes(), *pattern, rustix::io::Errno::INVAL)
            }
        };
        set_last_errno(errno);
        if checked {
            sash_emsg(
                operation.as_ptr(),
                operation.len() as i64,
                path.as_ptr(),
                path.len() as i64,
            );
        }
    }
}

fn absolute_prefix(directory: &[u8]) -> Result<Bytes, Failure<'_>> {
    let mut cwd_len = 0;
    let cwd = if directory.starts_with(b"/") {
        &b""[..]
    } else {
        let cwd_ptr = sash_getcwd(&mut cwd_len);
        if cwd_ptr.is_null() {
            let errno =
                unsafe { rustix::io::Errno::from_raw_os_error(crate::SASH_LAST_ERRNO as i32) };
            return Err(Failure::System(b"glob: getcwd", directory, errno));
        }
        unsafe { slice::from_raw_parts(cwd_ptr, cwd_len as usize) }
    };
    let capacity = cwd
        .len()
        .checked_add(directory.len())
        .and_then(|n| n.checked_add(1))
        .unwrap_or_else(|| sash_arith_overflow());
    let mut result = Bytes::new(capacity);
    let out = result.as_mut_slice();
    out[0] = b'/';
    let mut len = 1;
    for component in cwd
        .split(|&b| b == b'/')
        .chain(directory.split(|&b| b == b'/'))
    {
        match component {
            b"" | b"." => {}
            b".." => {
                len = out[..len]
                    .iter()
                    .rposition(|&b| b == b'/')
                    .unwrap_or(0)
                    .max(1);
            }
            _ => {
                if len > 1 {
                    out[len] = b'/';
                    len += 1;
                }
                out[len..len + component.len()].copy_from_slice(component);
                len += component.len();
            }
        }
    }
    result.len = len;
    unsafe { *result.ptr.add(len) = 0 };
    Ok(result)
}

fn matches(name: &[u8], pattern: &[u8]) -> bool {
    if name == b"." || name == b".." || (name.starts_with(b".") && !pattern.starts_with(b".")) {
        return false;
    }
    match pattern.iter().position(|&b| b == b'*') {
        Some(star) => {
            let prefix = &pattern[..star];
            let suffix = &pattern[star + 1..];
            name.len() >= prefix.len() + suffix.len()
                && name.starts_with(prefix)
                && name.ends_with(suffix)
        }
        None => name == pattern,
    }
}

fn select<'a>(pattern: &'a [u8], paths: &mut Paths) -> Result<(), Failure<'a>> {
    let (directory, filename) =
        glob_pattern::split(pattern).map_err(|reason| Failure::Pattern(pattern, reason))?;
    let mut storage = Bytes::new(directory.len());
    storage.as_mut_slice().copy_from_slice(directory);
    let path = unsafe { core::ffi::CStr::from_ptr(storage.ptr.cast()) };
    let fd = open(
        path,
        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::CLOEXEC,
        Mode::empty(),
    )
    .map_err(|e| Failure::System(b"glob: open", directory, e))?;
    let prefix = absolute_prefix(directory)?;
    let mut buffer = [mem::MaybeUninit::uninit(); 4096];
    let mut entries = RawDir::new(&fd, &mut buffer);
    let start = paths.len;
    while let Some(entry) = entries.next() {
        let entry = entry.map_err(|e| Failure::System(b"glob: read", directory, e))?;
        let name = entry.file_name().to_bytes();
        if matches(name, filename) {
            paths.push(prefix.as_slice(), name);
        }
    }
    paths.sort_from(start);
    Ok(())
}

/// `target` is a private empty batch; `patterns` is consumed, without its values.
#[no_mangle]
pub unsafe extern "C" fn sash_glob(
    target: *mut SashTable,
    patterns: *mut SashTable,
    checked: i64,
) -> i64 {
    let arguments = TableBatch(patterns);
    let mut paths = Paths::new();
    for i in 0..(*patterns).entry_count as usize {
        let entry = &*(*patterns).entries.add(i);
        let pattern = slice::from_raw_parts(entry.val_ptr, entry.val_len as usize);
        if let Err(failure) = select(pattern, &mut paths) {
            failure.record(checked != 0);
            drop(paths);
            drop(arguments);
            if checked != 0 {
                sash_abort(1);
            }
            return 1;
        }
    }
    paths.publish(target);
    0
}
