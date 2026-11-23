use std::time::Duration;

use duckylight::keyboard::Reconnect;
use duckylight::lock::{self, LockError};

fn tmp(name: &str) -> std::path::PathBuf {
    let p = std::env::temp_dir().join(format!("duckylight-test-{}-{name}", std::process::id()));
    let _ = std::fs::remove_file(&p);
    let _ = std::fs::remove_file(p.with_extension("pid"));
    p
}

#[test]
fn a_second_acquire_is_refused_with_the_holders_pid() {
    let p = tmp("held");
    let _first = lock::acquire_at(&p).unwrap();
    assert!(matches!(lock::acquire_at(&p), Err(LockError::Held(pid)) if pid == std::process::id()));
}

#[test]
fn a_lock_file_left_behind_by_a_dead_process_is_taken_over() {
    let p = tmp("stale");
    std::fs::write(&p, "").unwrap();
    std::fs::write(p.with_extension("pid"), "999999").unwrap(); // nobody holds the lock itself
    assert!(lock::acquire_at(&p).is_ok());
}

#[test]
fn a_dropped_lock_can_be_acquired_again() {
    let p = tmp("dropped");
    drop(lock::acquire_at(&p).unwrap());
    assert!(lock::acquire_at(&p).is_ok());
}

#[test]
fn reconnect_does_not_spin_when_no_device_is_there() {
    let mut probes = 0u32;
    let mut r = Reconnect::new(|| -> Result<duckylight::keyboard::fake::FakeKeyboard, String> {
        probes += 1;
        Err("nicht da".into())
    });
    for _ in 0..200 {
        assert!(r.get().is_none());
    }
    drop(r);
    assert_eq!(probes, 1);
}

#[test]
fn reconnect_finds_a_device_that_appears_later_and_drops_one_that_fails() {
    let mut present = false;
    let flag = std::cell::Cell::new(false);
    let mut r = Reconnect::with_interval(Duration::ZERO, || {
        if flag.get() { Ok(duckylight::keyboard::fake::FakeKeyboard::new()) } else { Err("nicht da".to_string()) }
    });
    assert!(r.get().is_none());
    flag.set(true);
    present |= r.get().is_some();
    assert!(present);
    r.lost("Kabel gezogen");
    flag.set(false);
    assert!(r.get().is_none());
    assert_eq!(r.last_error(), Some("nicht da"));
}
