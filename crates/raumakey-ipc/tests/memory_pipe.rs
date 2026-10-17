use std::io::{ErrorKind, Read, Write};
use std::thread;
use std::time::{Duration, Instant};

use raumakey_ipc::memory::duplex;

#[test]
fn bytes_written_on_one_end_are_read_on_the_other() {
    let (mut a, mut b) = duplex();
    a.write_all(b"hello").unwrap();
    b.write_all(b"world!").unwrap();
    let mut buf = [0_u8; 16];
    let n = b.read(&mut buf).unwrap();
    assert_eq!(&buf[..n], b"hello");
    let n = a.read(&mut buf).unwrap();
    assert_eq!(&buf[..n], b"world!");
}

#[test]
fn reads_return_buffered_bytes_in_order_across_calls() {
    let (mut a, mut b) = duplex();
    a.write_all(b"abc").unwrap();
    a.write_all(b"def").unwrap();
    let mut buf = [0_u8; 4];
    let n = b.read(&mut buf).unwrap();
    assert_eq!(&buf[..n], b"abcd");
    let n = b.read(&mut buf).unwrap();
    assert_eq!(&buf[..n], b"ef");
}

#[test]
fn dropping_one_end_closes_the_pipe_after_buffered_bytes() {
    let (mut a, mut b) = duplex();
    a.write_all(b"xy").unwrap();
    drop(a);
    let mut buf = [0_u8; 8];
    let n = b.read(&mut buf).unwrap();
    assert_eq!(&buf[..n], b"xy");
    assert_eq!(b.read(&mut buf).unwrap(), 0);
    let error = b.write(b"z").unwrap_err();
    assert_eq!(error.kind(), ErrorKind::BrokenPipe);
}

#[test]
fn read_timeouts_report_timed_out_and_zero_means_would_block() {
    let (mut a, _b) = duplex();
    let mut buf = [0_u8; 8];
    a.set_read_timeout(Some(Duration::from_millis(20)));
    let started = Instant::now();
    assert_eq!(a.read(&mut buf).unwrap_err().kind(), ErrorKind::TimedOut);
    assert!(started.elapsed() >= Duration::from_millis(15));
    a.set_read_timeout(Some(Duration::ZERO));
    assert_eq!(a.read(&mut buf).unwrap_err().kind(), ErrorKind::WouldBlock);
    a.set_read_timeout(Some(Duration::MAX));
    assert_eq!(a.read_timeout(), Some(Duration::MAX));
}

#[test]
fn blocking_read_wakes_when_the_peer_writes_from_another_thread() {
    let (mut a, mut b) = duplex();
    let writer = thread::spawn(move || {
        thread::sleep(Duration::from_millis(20));
        a.write_all(b"late").unwrap();
        a
    });
    let mut buf = [0_u8; 8];
    let n = b.read(&mut buf).unwrap();
    assert_eq!(&buf[..n], b"late");
    drop(writer.join().unwrap());
}

#[test]
fn close_wakes_a_blocked_reader_and_fails_later_writes() {
    let (a, mut b) = duplex();
    let closer = thread::spawn(move || {
        thread::sleep(Duration::from_millis(20));
        a.close();
        a
    });
    let mut buf = [0_u8; 8];
    assert_eq!(b.read(&mut buf).unwrap(), 0);
    let mut a = closer.join().unwrap();
    assert_eq!(a.write(b"x").unwrap_err().kind(), ErrorKind::BrokenPipe);
    assert_eq!(a.read(&mut buf).unwrap(), 0);
}
