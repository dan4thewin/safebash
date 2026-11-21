//! Scoped abandonment for a compiler-owned ERR call boundary.

use super::{
    capture_pop, close_reader, fd_backup_abort, reap_reader, sash_alloc, sash_close, sash_free_ptr,
    sash_restore_fd, CaptureFrame, SashReader, CAPTURE_SCOPE, CAPTURE_TOP, SASH_READERS,
    TERMINAL_ERR, TERMINAL_STATE,
};
use core::ptr::null_mut;

struct Backup {
    previous: *mut Backup,
    fd: i64,
    saved: i64,
}

struct Attempt {
    readers: *mut SashReader,
    captures: *mut CaptureFrame,
    backups: *mut Backup,
}

static mut ACTIVE: Option<Attempt> = None;

#[no_mangle]
pub extern "C" fn sash_attempt_begin() {
    assert_eq!(
        TERMINAL_STATE.load(core::sync::atomic::Ordering::Relaxed),
        TERMINAL_ERR,
        "attempts belong to ERR"
    );
    unsafe {
        assert!(ACTIVE.is_none(), "attempt boundaries cannot nest");
        ACTIVE = Some(Attempt {
            readers: SASH_READERS,
            captures: CAPTURE_TOP[CAPTURE_SCOPE],
            backups: null_mut(),
        });
    }
}

#[no_mangle]
pub extern "C" fn sash_attempt_end() {
    unsafe {
        let attempt = ACTIVE.take().expect("attempt end without start");
        assert!(
            attempt.backups.is_null()
                && SASH_READERS == attempt.readers
                && CAPTURE_TOP[CAPTURE_SCOPE] == attempt.captures,
            "normal attempt return must close its scoped resources"
        );
    }
}

#[no_mangle]
pub extern "C" fn sash_attempt_abandon() {
    unsafe {
        // Deactivate before cleanup: restoration cannot recover into itself.
        let attempt = ACTIVE.take().expect("attempt abandonment without start");
        let mut backup = attempt.backups;
        while let Some(saved) = backup.as_ref() {
            sash_restore_fd(saved.saved, saved.fd);
            let previous = saved.previous;
            sash_free_ptr(backup.cast());
            backup = previous;
        }
        while CAPTURE_TOP[CAPTURE_SCOPE] != attempt.captures {
            let frame = capture_pop();
            if frame.buffered {
                sash_free_ptr(frame.ptr);
            } else if sash_close(frame.fd) < 0 {
                fd_backup_abort(b"close abandoned capture");
            }
        }
        let head = SASH_READERS;
        SASH_READERS = attempt.readers;
        let mut scope = head;
        while scope != attempt.readers {
            let reader = scope
                .as_mut()
                .expect("attempt reader bookmark must remain live");
            close_reader(reader);
            scope = reader.previous;
        }
        scope = head;
        while scope != attempt.readers {
            let reader = scope
                .as_mut()
                .expect("attempt reader bookmark must remain live");
            reap_reader(reader);
            scope = reader.previous;
        }
    }
}

pub(super) fn saved(fd: i64, saved: i64) {
    unsafe {
        let Some(attempt) = ACTIVE.as_mut() else {
            return;
        };
        let entry = sash_alloc(core::mem::size_of::<Backup>()).cast::<Backup>();
        entry.write(Backup {
            previous: attempt.backups,
            fd,
            saved,
        });
        attempt.backups = entry;
    }
}

pub(super) fn released(fd: i64, saved: i64) {
    unsafe {
        let Some(attempt) = ACTIVE.as_mut() else {
            return;
        };
        let mut link = &raw mut attempt.backups;
        while let Some(entry) = (*link).as_mut() {
            if entry.fd == fd && entry.saved == saved {
                let old = *link;
                *link = entry.previous;
                sash_free_ptr(old.cast());
                return;
            }
            link = &raw mut entry.previous;
        }
        panic!("attempt released a descriptor backup it does not own");
    }
}

pub(super) fn child() {
    unsafe {
        ACTIVE = None;
    }
}
