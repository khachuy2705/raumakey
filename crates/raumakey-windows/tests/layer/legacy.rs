//! `legacy` (feature `evkey-compat`): the Windows named-object adapter of the legacy channel.
//!
//! Every test uses its own isolated object names (`Local\Raumakey-Test-<pid>-<n>-…`); the
//! legacy product names are never created, opened, or probed here.

use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use raumakey_ipc::legacy::{
    BLOCK_SIZE, LegacyChannel, LegacyClient, LegacyEdit, LegacyError, LegacyEvent, LegacyObject,
    LegacyRequest, LegacyServer, LegacyServerConfig, LockResult, MUTEX_WAIT, MutexOrigin,
    OFF_OUTPUT, RESPONSE_WAIT, ResetOutcome, ShutdownOutcome, WaitResult, activate_controller,
};
use raumakey_windows::legacy::{LegacyNamedObjects, LegacyObjectNames};
use raumakey_windows::strings::WideString;
use windows::Win32::Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE};
use windows::Win32::System::Memory::{CreateFileMappingW, PAGE_READWRITE, SEC_RESERVE};
use windows::Win32::System::Threading::{CreateEventW, OpenEventW, SYNCHRONIZATION_ACCESS_RIGHTS};

use crate::support::unique_suffix;

/// `ERROR_NOT_OWNER`.
const ERROR_NOT_OWNER: u32 = 288;
/// `READ_CONTROL`.
const READ_CONTROL: u32 = 0x0002_0000;

fn names() -> LegacyObjectNames {
    LegacyObjectNames::isolated(&unique_suffix()).unwrap()
}

fn channel(names: &LegacyObjectNames) -> LegacyNamedObjects {
    LegacyNamedObjects::new(names.clone()).unwrap()
}

/// A kernel handle created by the test itself (an object "another process" holds).
struct RawHandle(HANDLE);

impl Drop for RawHandle {
    fn drop(&mut self) {
        // SAFETY: the handle was created by this test and is closed once.
        let _ = unsafe { CloseHandle(self.0) };
    }
}

fn raw_event(name: &str) -> RawHandle {
    let wide = WideString::new(name).unwrap();
    // SAFETY: the name outlives the call; the handle is owned by the result.
    RawHandle(unsafe { CreateEventW(None, false, false, wide.as_pcwstr()) }.unwrap())
}

fn raw_mapping(name: &str, size: u32, reserve: bool) -> RawHandle {
    let wide = WideString::new(name).unwrap();
    let protection = if reserve {
        PAGE_READWRITE | SEC_RESERVE
    } else {
        PAGE_READWRITE
    };
    // SAFETY: a pagefile-backed mapping; the name outlives the call; the handle is owned by
    // the result.
    RawHandle(
        unsafe {
            CreateFileMappingW(
                INVALID_HANDLE_VALUE,
                None,
                protection,
                0,
                size,
                wide.as_pcwstr(),
            )
        }
        .unwrap(),
    )
}

#[test]
fn production_names_are_the_session_local_legacy_names() {
    let legacy = LegacyObjectNames::legacy();
    for object in LegacyObject::ALL {
        assert_eq!(legacy.name(object), format!("Local\\{}", object.name()));
    }
    let isolated = names();
    let mut seen = Vec::new();
    for object in LegacyObject::ALL {
        let name = isolated.name(object);
        assert!(name.starts_with("Local\\Raumakey-Test-"), "{name}");
        assert!(!name.contains(object.name()), "{name}");
        assert!(!seen.contains(&name.to_owned()), "{name}");
        seen.push(name.to_owned());
    }
    for bad in ["", "a b", "a\\b", "a.b", &"x".repeat(65)] {
        assert!(LegacyObjectNames::isolated(bad).is_err(), "{bad:?}");
    }
}

#[test]
fn the_controller_creates_its_objects_once_and_activation_fails_closed() {
    let names = names();
    let controller = channel(&names);
    for object in LegacyObject::ALL {
        assert!(!controller.object_exists(object).unwrap(), "{object:?}");
    }
    activate_controller(&controller).unwrap();
    for object in LegacyObject::CONTROLLER_OWNED {
        assert!(controller.object_exists(object).unwrap(), "{object:?}");
    }
    assert!(!controller.object_exists(LegacyObject::Mutex).unwrap());

    let rival = channel(&names);
    assert_eq!(
        activate_controller(&rival),
        Err(LegacyError::ConflictingObject(LegacyObject::SharedMemory))
    );
    assert_eq!(
        rival.create_controller_objects(),
        Err(LegacyError::ConflictingObject(LegacyObject::SharedMemory))
    );
    assert_eq!(
        controller.create_controller_objects(),
        Err(LegacyError::ConflictingObject(LegacyObject::SharedMemory))
    );
    let mut buffer = [0_u8; 4];
    assert_eq!(
        rival.read_shared(0, &mut buffer),
        Err(LegacyError::ObjectMissing(LegacyObject::SharedMemory))
    );

    controller.close();
    for object in LegacyObject::ALL {
        assert!(!controller.object_exists(object).unwrap(), "{object:?}");
    }
    // Once the last handle is gone a controller may start again.
    activate_controller(&rival).unwrap();
}

#[test]
fn any_existing_object_blocks_activation_and_nothing_is_created() {
    for object in LegacyObject::CONTROLLER_OWNED {
        let names = names();
        let squatter = match object {
            LegacyObject::SharedMemory => raw_mapping(names.name(object), BLOCK_SIZE as u32, false),
            _ => raw_event(names.name(object)),
        };
        let controller = channel(&names);
        assert!(controller.object_exists(object).unwrap());
        assert_eq!(
            activate_controller(&controller),
            Err(LegacyError::ConflictingObject(object))
        );
        for other in LegacyObject::ALL
            .into_iter()
            .filter(|other| *other != object)
        {
            assert!(!controller.object_exists(other).unwrap(), "{other:?}");
        }
        drop(squatter);
        assert!(!controller.object_exists(object).unwrap());
    }
}

#[test]
fn a_name_taken_by_another_object_type_counts_as_existing() {
    let names = names();
    // An event that carries the name of the mapping.
    let _squatter = raw_event(names.name(LegacyObject::SharedMemory));
    let controller = channel(&names);
    assert!(
        controller
            .object_exists(LegacyObject::SharedMemory)
            .unwrap()
    );
    assert_eq!(
        controller.create_controller_objects(),
        Err(LegacyError::ConflictingObject(LegacyObject::SharedMemory))
    );
    assert!(
        !controller
            .object_exists(LegacyObject::RequestEvent)
            .unwrap()
    );
}

#[test]
fn tips_open_or_create_the_mutex_in_either_start_order() {
    let names = names();
    let tip = channel(&names);
    assert_eq!(
        tip.open_controller_objects(),
        Err(LegacyError::ObjectMissing(LegacyObject::SharedMemory))
    );
    assert_eq!(tip.open_or_create_mutex().unwrap(), MutexOrigin::Created);
    let second = channel(&names);
    assert_eq!(second.open_or_create_mutex().unwrap(), MutexOrigin::Opened);
    // A mutex created by a TIP that started first is not a conflict for the controller.
    let controller = channel(&names);
    activate_controller(&controller).unwrap();
    tip.open_controller_objects().unwrap();
    second.open_controller_objects().unwrap();
}

#[test]
fn reopening_replaces_handles_and_a_tip_keeps_objects_alive() {
    let names = names();
    let controller = channel(&names);
    activate_controller(&controller).unwrap();
    let tip = channel(&names);
    tip.open_controller_objects().unwrap();
    tip.open_controller_objects().unwrap();
    tip.open_or_create_mutex().unwrap();
    tip.open_or_create_mutex().unwrap();
    controller.close();
    // The TIP still holds the controller's objects.
    for object in LegacyObject::ALL {
        assert!(tip.object_exists(object).unwrap(), "{object:?}");
    }
    tip.close();
    // One close released everything: the second open replaced the first handles.
    for object in LegacyObject::ALL {
        assert!(!tip.object_exists(object).unwrap(), "{object:?}");
    }
    assert_eq!(
        tip.open_controller_objects(),
        Err(LegacyError::ObjectMissing(LegacyObject::SharedMemory))
    );
}

#[test]
fn shared_memory_is_zeroed_bounded_and_shared_between_endpoints() {
    let names = names();
    let controller = channel(&names);
    activate_controller(&controller).unwrap();
    let tip = channel(&names);
    tip.open_controller_objects().unwrap();

    let mut block = vec![0xAA_u8; BLOCK_SIZE];
    tip.read_shared(0, &mut block).unwrap();
    assert!(block.iter().all(|byte| *byte == 0));

    controller.write_shared(OFF_OUTPUT, &[1, 2, 3, 4]).unwrap();
    let mut read = [0_u8; 4];
    tip.read_shared(OFF_OUTPUT, &mut read).unwrap();
    assert_eq!(read, [1, 2, 3, 4]);
    tip.write_shared(BLOCK_SIZE - 1, &[9]).unwrap();
    let mut last = [0_u8; 1];
    controller.read_shared(BLOCK_SIZE - 1, &mut last).unwrap();
    assert_eq!(last, [9]);
    tip.read_shared(BLOCK_SIZE, &mut []).unwrap();

    let mut two = [0_u8; 2];
    assert_eq!(
        tip.read_shared(BLOCK_SIZE - 1, &mut two),
        Err(LegacyError::OutOfBounds)
    );
    assert_eq!(
        tip.write_shared(BLOCK_SIZE, &[1]),
        Err(LegacyError::OutOfBounds)
    );
    assert_eq!(
        tip.read_shared(usize::MAX, &mut two),
        Err(LegacyError::OutOfBounds)
    );
}

#[test]
fn events_are_auto_reset_and_waits_honour_their_timeouts() {
    let names = names();
    let controller = channel(&names);
    activate_controller(&controller).unwrap();
    let tip = channel(&names);
    tip.open_controller_objects().unwrap();

    assert_eq!(
        controller
            .wait(LegacyEvent::Request, Duration::ZERO)
            .unwrap(),
        WaitResult::TimedOut
    );
    tip.signal(LegacyEvent::Request).unwrap();
    assert_eq!(
        controller
            .wait(LegacyEvent::Request, Duration::ZERO)
            .unwrap(),
        WaitResult::Signaled
    );
    assert_eq!(
        controller
            .wait(LegacyEvent::Request, Duration::ZERO)
            .unwrap(),
        WaitResult::TimedOut,
        "a successful wait resets the event"
    );
    controller.signal(LegacyEvent::Response).unwrap();
    assert_eq!(
        tip.wait(LegacyEvent::Response, Duration::from_secs(2))
            .unwrap(),
        WaitResult::Signaled
    );
    let started = Instant::now();
    assert_eq!(
        tip.wait(LegacyEvent::Response, RESPONSE_WAIT).unwrap(),
        WaitResult::TimedOut
    );
    let waited = started.elapsed();
    assert!(waited >= Duration::from_millis(30), "{waited:?}");
    assert!(waited < Duration::from_secs(2), "{waited:?}");
    let first = tip.now_ms();
    assert!(controller.now_ms() >= first, "one monotonic clock");
}

#[test]
fn the_mutex_serializes_threads_and_reports_abandonment() {
    let names = names();
    let tip = Arc::new(channel(&names));
    tip.open_or_create_mutex().unwrap();
    assert_eq!(tip.lock_mutex(MUTEX_WAIT).unwrap(), LockResult::Acquired);

    let other = Arc::clone(&tip);
    let started = Instant::now();
    let contended = thread::spawn(move || other.lock_mutex(MUTEX_WAIT).unwrap())
        .join()
        .unwrap();
    let waited = started.elapsed();
    assert_eq!(contended, LockResult::TimedOut);
    assert!(waited >= Duration::from_millis(60), "{waited:?}");
    assert!(waited < Duration::from_secs(2), "{waited:?}");
    tip.unlock_mutex().unwrap();
    assert_eq!(
        tip.unlock_mutex(),
        Err(LegacyError::Channel {
            code: ERROR_NOT_OWNER
        })
    );

    // A thread that exits while owning the mutex abandons it.
    let other = Arc::clone(&tip);
    let taken = thread::spawn(move || other.lock_mutex(MUTEX_WAIT).unwrap())
        .join()
        .unwrap();
    assert_eq!(taken, LockResult::Acquired);
    assert_eq!(tip.lock_mutex(MUTEX_WAIT).unwrap(), LockResult::Abandoned);
    tip.unlock_mutex().unwrap();
}

#[test]
fn after_close_every_call_fails_and_in_flight_waits_finish() {
    let names = names();
    let controller = Arc::new(channel(&names));
    activate_controller(&*controller).unwrap();
    controller.open_or_create_mutex().unwrap();

    let waiter = Arc::clone(&controller);
    let in_flight =
        thread::spawn(move || waiter.wait(LegacyEvent::Request, Duration::from_millis(300)));
    thread::sleep(Duration::from_millis(50));
    controller.close();
    let result = in_flight.join().unwrap();
    assert!(
        matches!(
            result,
            Ok(WaitResult::TimedOut) | Err(LegacyError::ObjectMissing(_))
        ),
        "{result:?}"
    );

    let mut buffer = [0_u8; 4];
    let memory = Err(LegacyError::ObjectMissing(LegacyObject::SharedMemory));
    assert_eq!(controller.read_shared(0, &mut buffer), memory);
    assert_eq!(controller.write_shared(0, &buffer), memory);
    assert_eq!(
        controller.signal(LegacyEvent::Request),
        Err(LegacyError::ObjectMissing(LegacyObject::RequestEvent))
    );
    assert_eq!(
        controller.wait(LegacyEvent::Response, Duration::ZERO),
        Err(LegacyError::ObjectMissing(LegacyObject::ResponseEvent))
    );
    let mutex = Err(LegacyError::ObjectMissing(LegacyObject::Mutex));
    assert_eq!(
        controller.lock_mutex(Duration::ZERO),
        mutex.map(|()| LockResult::Acquired)
    );
    assert_eq!(controller.unlock_mutex(), mutex);
    controller.close();
    for object in LegacyObject::ALL {
        assert!(!controller.object_exists(object).unwrap(), "{object:?}");
    }
}

#[test]
fn objects_carry_the_user_only_dacl() {
    use raumakey_windows::security::kernel_object_sddl;
    use raumakey_windows::sid::current_user_sid_string;

    let names = names();
    let controller = channel(&names);
    activate_controller(&controller).unwrap();
    let wide = WideString::new(names.name(LegacyObject::RequestEvent)).unwrap();
    // SAFETY: the name outlives the call; the handle is owned below.
    let event = RawHandle(
        unsafe {
            OpenEventW(
                SYNCHRONIZATION_ACCESS_RIGHTS(READ_CONTROL),
                false,
                wide.as_pcwstr(),
            )
        }
        .unwrap(),
    );
    let sddl = kernel_object_sddl(event.0).unwrap();
    let sid = current_user_sid_string().unwrap();
    assert!(sddl.contains("D:P(A;;"), "{sddl}");
    assert_eq!(sddl.matches("(A;").count(), 1, "{sddl}");
    assert!(sddl.contains(&sid), "{sddl}");
}

#[test]
fn undersized_or_uncommitted_mappings_are_refused() {
    for (size, reserve) in [(16_u32, false), (0x2000, true)] {
        let names = names();
        let _mapping = raw_mapping(names.name(LegacyObject::SharedMemory), size, reserve);
        let _request = raw_event(names.name(LegacyObject::RequestEvent));
        let _response = raw_event(names.name(LegacyObject::ResponseEvent));
        let tip = channel(&names);
        let error = tip.open_controller_objects().unwrap_err();
        assert!(
            matches!(error, LegacyError::Channel { .. }),
            "{size:#x} {reserve}: {error:?}"
        );
        let mut buffer = [0_u8; 4];
        assert_eq!(
            tip.read_shared(0, &mut buffer),
            Err(LegacyError::ObjectMissing(LegacyObject::SharedMemory))
        );
        assert_eq!(
            tip.signal(LegacyEvent::Request),
            Err(LegacyError::ObjectMissing(LegacyObject::RequestEvent))
        );
    }
}

/// Runs `op`, reconnecting and retrying when a loaded test machine misses the 50 ms window.
/// (Real TIPs never retry an exchange; this only keeps the test independent of scheduling.)
fn with_retries<T>(
    client: &mut LegacyClient<LegacyNamedObjects>,
    mut op: impl FnMut(&mut LegacyClient<LegacyNamedObjects>) -> Result<T, LegacyError>,
) -> T {
    let mut last = None;
    for _ in 0..20 {
        if !client.is_connected()
            && let Err(error) = client.connect()
        {
            last = Some(error);
            thread::sleep(Duration::from_millis(20));
            continue;
        }
        match op(client) {
            Ok(value) => return value,
            Err(error) => last = Some(error),
        }
    }
    panic!("legacy exchange kept failing: {last:?}");
}

#[test]
fn a_full_exchange_runs_over_real_kernel_objects() {
    let names = names();
    let server =
        Arc::new(LegacyServer::start(channel(&names), LegacyServerConfig::default()).unwrap());
    let worker = {
        let server = Arc::clone(&server);
        thread::spawn(move || {
            server.run(|request| match request {
                LegacyRequest::ProcessChar { unit, .. } if unit == u16::from(b'a') => LegacyEdit {
                    delete_before: 1,
                    insert_utf16: "â".encode_utf16().collect(),
                },
                _ => LegacyEdit::empty(),
            })
        })
    };

    let mut client = LegacyClient::new(channel(&names));
    // `with_retries` connects (op 5 sync) before running the operation.
    with_retries(&mut client, |_| Ok(()));
    assert!(client.is_connected());
    let edit = with_retries(&mut client, |client| {
        client.process_char(u16::from(b'a'), false, false)
    });
    assert_eq!(edit.delete_before, 1);
    assert_eq!(edit.insert_utf16, "â".encode_utf16().collect::<Vec<_>>());
    let edit = with_retries(&mut client, |client| {
        client.process_char(u16::from(b'b'), true, false)
    });
    assert!(edit.is_empty());
    let reset = with_retries(&mut client, LegacyClient::reset);
    assert_eq!(reset, ResetOutcome::Remote);
    drop(client);

    assert_eq!(server.shutdown(), ShutdownOutcome::Completed);
    worker.join().unwrap().unwrap();
    let probe = channel(&names);
    for object in LegacyObject::ALL {
        assert!(!probe.object_exists(object).unwrap(), "{object:?}");
    }
}
