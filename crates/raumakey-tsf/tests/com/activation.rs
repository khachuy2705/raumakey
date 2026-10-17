//! Activation and deactivation of the text service against the real TSF thread manager of
//! this process (plan §10.3, design §12).
//!
//! TSF never activates the text service here: it is not registered. The real thread manager
//! advises the sinks that applications may advise too (thread manager events, thread focus,
//! compartment events), but it refuses the key event sink of a text service that it did not
//! activate. [`the_real_thread_manager_refuses_the_key_sink`] pins that behavior; the other
//! tests run through [`ThreadMgrShim`], which stands in for the keystroke manager only (see
//! `shim`). Reference counts show which sinks TSF still holds.

use raumakey_windows::com_server::object_count;
use windows::Win32::Foundation::{E_INVALIDARG, S_OK};
use windows::Win32::UI::TextServices::{
    ITfTextInputProcessor, ITfTextInputProcessorEx, ITfThreadMgr, TF_TMAE_UIELEMENTENABLEDONLY,
};

use crate::shim::{Callback, Event, FailAt, ThreadMgrShim, activated_thread_manager};
use crate::support::{on_sta, references, serial, text_service};

/// The four advises of a successful activation through the shim, with the cookies of the two
/// thread manager sinks, and the look-up of the focused document that follows them.
fn advised(events: &[Event], client_id: u32) -> (u32, u32) {
    match events {
        [
            Event::AdviseSink {
                sink: "ITfThreadMgrEventSink",
                cookie: Some(thread_events),
            },
            Event::AdviseSink {
                sink: "ITfThreadFocusSink",
                cookie: Some(thread_focus),
            },
            Event::AdviseKeyEventSink {
                client_id: key_client,
                foreground: true,
            },
            Event::GetCompartment {
                keyboard_open_close: true,
            },
            Event::GetFocus,
        ] if *key_client == client_id => (*thread_events, *thread_focus),
        other => panic!("unexpected activation calls: {other:?}"),
    }
}

/// A call back into the text service that succeeded; `eaten` for a key test.
fn callback(method: &'static str, eaten: Option<bool>) -> Callback {
    Callback {
        method,
        code: S_OK,
        eaten,
    }
}

#[test]
fn the_real_thread_manager_refuses_the_key_sink() {
    let _serial = serial();
    on_sta(|| {
        let (thread_mgr, client_id) = activated_thread_manager();
        let service = text_service();
        let baseline = references(&service);
        // SAFETY: a live text service and thread manager of this thread.
        let result = unsafe { service.ActivateEx(&thread_mgr, client_id, 0) };
        assert_eq!(
            result.err().map(|error| error.code()),
            Some(E_INVALIDARG),
            "TSF refuses the key event sink of a text service that it did not activate"
        );
        assert_eq!(
            references(&service),
            baseline,
            "the sinks advised before the key event sink were unadvised again"
        );
        // SAFETY: as above.
        unsafe { service.Deactivate() }.expect("Deactivate after a failed activation");
        drop(service);
        assert_eq!(
            object_count(),
            0,
            "TSF holds no reference to the text service"
        );
        // SAFETY: the thread manager activated above.
        unsafe { thread_mgr.Deactivate() }.expect("the thread manager is deactivated");
    });
}

#[test]
fn activation_advises_every_sink_and_deactivation_releases_every_one() {
    let _serial = serial();
    on_sta(|| {
        let (real, client_id) = activated_thread_manager();
        let shim = ThreadMgrShim::new(&real, client_id);
        let thread_mgr: ITfThreadMgr = shim.to_interface();
        let service = text_service();
        let baseline = references(&service);

        // SAFETY: a live text service and thread manager of this thread.
        unsafe { service.ActivateEx(&thread_mgr, client_id, TF_TMAE_UIELEMENTENABLEDONLY) }
            .expect("ActivateEx");
        let (thread_events, thread_focus) = advised(&shim.events.borrow(), client_id);
        assert!(
            references(&service) >= baseline + 4,
            "TSF and the keystroke manager hold the four sinks"
        );
        assert!(shim.key_sink.borrow().is_some());
        assert_eq!(
            *shim.callbacks.borrow(),
            [
                callback("OnSetFocus(TRUE)", None),
                callback("OnTestKeyDown", Some(false)),
            ],
            "the calls back from inside ActivateEx succeeded and let the key through"
        );

        shim.events.borrow_mut().clear();
        // SAFETY: as above.
        unsafe { service.Deactivate() }.expect("Deactivate");
        assert_eq!(
            *shim.events.borrow(),
            [
                Event::UnadviseKeyEventSink { client_id },
                Event::UnadviseSink {
                    cookie: thread_focus
                },
                Event::UnadviseSink {
                    cookie: thread_events
                },
            ],
            "every sink is unadvised, last first (the compartment sink with real TSF)"
        );
        assert_eq!(references(&service), baseline, "no sink is held any more");
        assert!(shim.key_sink.borrow().is_none());
        assert_eq!(
            shim.callbacks.borrow()[2..],
            [callback("OnSetFocus(FALSE)", None)],
            "the call back from inside Deactivate succeeded"
        );

        drop(service);
        assert_eq!(
            object_count(),
            0,
            "no reference to the text service is left"
        );
        drop(thread_mgr);
        // SAFETY: the thread manager activated above.
        unsafe { real.Deactivate() }.expect("the thread manager is deactivated");
    });
}

#[test]
fn a_failed_step_leaves_nothing_advised_and_its_error_is_returned() {
    let _serial = serial();
    on_sta(|| {
        let (real, client_id) = activated_thread_manager();
        for step in FailAt::ALL {
            let shim = ThreadMgrShim::new(&real, client_id);
            shim.fail.set(Some(step));
            let thread_mgr: ITfThreadMgr = shim.to_interface();
            let service = text_service();
            let baseline = references(&service);
            // SAFETY: a live text service and thread manager of this thread.
            let result = unsafe { service.ActivateEx(&thread_mgr, client_id, 0) };
            assert_eq!(
                result.err().map(|error| error.code()),
                Some(step.code()),
                "{step:?}"
            );
            assert_eq!(
                references(&service),
                baseline,
                "{step:?}: a sink is still held"
            );
            assert!(shim.key_sink.borrow().is_none(), "{step:?}");
            // SAFETY: as above.
            unsafe { service.Deactivate() }.expect("Deactivate after a failed activation");
            assert_eq!(references(&service), baseline, "{step:?}");
            drop((service, thread_mgr));
            assert_eq!(object_count(), 0, "{step:?}");
        }
        // SAFETY: the thread manager activated above.
        unsafe { real.Deactivate() }.expect("the thread manager is deactivated");
    });
}

#[test]
fn deactivation_is_idempotent_and_activation_can_follow_it() {
    let _serial = serial();
    on_sta(|| {
        let (real, client_id) = activated_thread_manager();
        let shim = ThreadMgrShim::new(&real, client_id);
        let thread_mgr: ITfThreadMgr = shim.to_interface();
        let service = text_service();
        let processor: ITfTextInputProcessor = service.clone().into();
        let baseline = references(&service);

        // SAFETY: calls on a live text service and thread manager of this thread.
        unsafe {
            processor
                .Deactivate()
                .expect("Deactivate before any activation");
            service
                .ActivateEx(&thread_mgr, client_id, 0)
                .expect("ActivateEx");
            processor.Deactivate().expect("Deactivate");
            shim.events.borrow_mut().clear();
            processor.Deactivate().expect("a second Deactivate");
            assert!(shim.events.borrow().is_empty(), "nothing left to unadvise");
            assert_eq!(references(&service), baseline);

            processor
                .Activate(&thread_mgr, client_id)
                .expect("Activate after Deactivate");
            // `Activate` advises the same four sinks as `ActivateEx`.
            let _cookies = advised(&shim.events.borrow(), client_id);
            processor.Deactivate().expect("Deactivate");
        }
        assert_eq!(references(&service), baseline);
        drop((processor, service, thread_mgr));
        assert_eq!(object_count(), 0);
        // SAFETY: the thread manager activated above.
        unsafe { real.Deactivate() }.expect("the thread manager is deactivated");
    });
}

#[test]
fn a_second_activation_replaces_the_first_without_leaking_its_sinks() {
    let _serial = serial();
    on_sta(|| {
        let (real, client_id) = activated_thread_manager();
        let shim = ThreadMgrShim::new(&real, client_id);
        let thread_mgr: ITfThreadMgr = shim.to_interface();
        let service = text_service();
        let baseline = references(&service);
        // SAFETY: calls on a live text service and thread manager of this thread.
        unsafe {
            service
                .ActivateEx(&thread_mgr, client_id, 0)
                .expect("ActivateEx");
            let once = references(&service);
            service
                .ActivateEx(&thread_mgr, client_id, 0)
                .expect("a second ActivateEx");
            assert_eq!(references(&service), once, "the first sinks were unadvised");
            service.Deactivate().expect("Deactivate");
        }
        assert_eq!(references(&service), baseline);
        drop((service, thread_mgr));
        assert_eq!(object_count(), 0);
        // SAFETY: the thread manager activated above.
        unsafe { real.Deactivate() }.expect("the thread manager is deactivated");
    });
}

#[test]
fn a_null_thread_manager_is_an_invalid_argument() {
    let _serial = serial();
    on_sta(|| {
        let service: ITfTextInputProcessorEx = text_service();
        let baseline = references(&service);
        // SAFETY: a live text service; the thread manager is null on purpose.
        let result = unsafe { service.ActivateEx(None::<&ITfThreadMgr>, 1, 0) };
        assert_eq!(result.err().map(|error| error.code()), Some(E_INVALIDARG));
        assert_eq!(references(&service), baseline);
        drop(service);
        assert_eq!(object_count(), 0);
    });
}
