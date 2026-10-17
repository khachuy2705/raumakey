//! The text service object: creation through the class factory, its interfaces, and what its
//! sinks do while it is not activated.

use raumakey_windows::com_server::object_count;
use windows::Win32::Foundation::{CLASS_E_NOAGGREGATION, E_NOINTERFACE, LPARAM, WPARAM};
use windows::Win32::System::Com::IAgileObject;
use windows::Win32::UI::TextServices::{
    GUID_COMPARTMENT_KEYBOARD_OPENCLOSE, ITfCompartmentEventSink, ITfContext, ITfDocumentMgr,
    ITfEditRecord, ITfKeyEventSink, ITfTextEditSink, ITfTextInputProcessor,
    ITfTextInputProcessorEx, ITfThreadFocusSink, ITfThreadMgrEventSink,
};
use windows::core::{GUID, IUnknown, Interface};

use crate::support::{class_factory, on_sta, references, serial, text_service};

/// `IMarshal` (`{00000003-0000-0000-C000-000000000046}`): an object that answers it with the
/// free-threaded marshaler claims to be callable from any apartment.
const IID_IMARSHAL: GUID = GUID::from_u128(0x0000_0003_0000_0000_c000_0000_0000_0046);

#[test]
fn aggregation_is_rejected() {
    let _serial = serial();
    on_sta(|| {
        let factory = class_factory();
        let outer: IUnknown = factory.cast().expect("the factory is an IUnknown");
        // SAFETY: an in-process class factory; `outer` is a live IUnknown.
        let result: windows::core::Result<IUnknown> = unsafe { factory.CreateInstance(&outer) };
        assert_eq!(
            result.err().map(|error| error.code()),
            Some(CLASS_E_NOAGGREGATION)
        );
        assert_eq!(object_count(), 1, "only the factory exists");
        drop((outer, factory));
        assert_eq!(object_count(), 0);
    });
}

#[test]
fn one_object_answers_for_the_processor_and_every_sink() {
    let _serial = serial();
    on_sta(|| {
        let service = text_service();
        assert_eq!(object_count(), 1, "the text service counts as an object");
        let identity: IUnknown = service.cast().expect("IUnknown");
        let interfaces: [(&str, windows::core::Result<IUnknown>); 7] = [
            (
                "ITfTextInputProcessorEx",
                service.cast::<ITfTextInputProcessorEx>().map(Into::into),
            ),
            (
                "ITfTextInputProcessor",
                service.cast::<ITfTextInputProcessor>().map(Into::into),
            ),
            (
                "ITfKeyEventSink",
                service.cast::<ITfKeyEventSink>().map(Into::into),
            ),
            (
                "ITfThreadMgrEventSink",
                service.cast::<ITfThreadMgrEventSink>().map(Into::into),
            ),
            (
                "ITfTextEditSink",
                service.cast::<ITfTextEditSink>().map(Into::into),
            ),
            (
                "ITfThreadFocusSink",
                service.cast::<ITfThreadFocusSink>().map(Into::into),
            ),
            (
                "ITfCompartmentEventSink",
                service.cast::<ITfCompartmentEventSink>().map(Into::into),
            ),
        ];
        for (name, interface) in interfaces {
            let interface = interface.unwrap_or_else(|error| panic!("{name}: {error:?}"));
            // COM identity: every interface leads back to the same IUnknown.
            let back: IUnknown = interface.cast().expect("IUnknown");
            assert_eq!(back.as_raw(), identity.as_raw(), "{name}");
        }
        assert_eq!(object_count(), 1, "still one object");
        assert_eq!(
            references(&service),
            2,
            "the test holds `service` and `identity`"
        );
        drop((identity, service));
        assert_eq!(object_count(), 0);
    });
}

#[test]
fn the_text_service_stays_in_its_apartment() {
    let _serial = serial();
    on_sta(|| {
        let service = text_service();
        assert_eq!(
            service
                .cast::<IAgileObject>()
                .err()
                .map(|error| error.code()),
            Some(E_NOINTERFACE),
            "the object is not agile"
        );
        let mut marshal = core::ptr::null_mut();
        // SAFETY: a live object, a GUID on this frame, and a writable result pointer.
        let code = unsafe { service.query(&IID_IMARSHAL, &mut marshal) };
        assert_eq!(code, E_NOINTERFACE, "no free-threaded marshaler");
        assert!(marshal.is_null());
    });
}

#[test]
fn without_an_activation_every_key_reaches_the_application_and_the_sinks_accept_their_calls() {
    let _serial = serial();
    on_sta(|| {
        let service = text_service();
        let keys: ITfKeyEventSink = service.cast().expect("ITfKeyEventSink");
        let letter_a = WPARAM(0x41);
        let first_press = LPARAM(0x001E_0001);
        // SAFETY: calls on a live in-process object; the context may be null.
        unsafe {
            keys.OnSetFocus(true).expect("OnSetFocus");
            for (name, eaten) in [
                (
                    "OnTestKeyDown",
                    keys.OnTestKeyDown(None::<&ITfContext>, letter_a, first_press),
                ),
                (
                    "OnKeyDown",
                    keys.OnKeyDown(None::<&ITfContext>, letter_a, first_press),
                ),
                (
                    "OnTestKeyUp",
                    keys.OnTestKeyUp(None::<&ITfContext>, letter_a, first_press),
                ),
                (
                    "OnKeyUp",
                    keys.OnKeyUp(None::<&ITfContext>, letter_a, first_press),
                ),
                (
                    "OnPreservedKey",
                    keys.OnPreservedKey(None::<&ITfContext>, &GUID::zeroed()),
                ),
            ] {
                let eaten = eaten.unwrap_or_else(|error| panic!("{name}: {error:?}"));
                assert!(!eaten.as_bool(), "{name} ate the key");
            }
            keys.OnSetFocus(false).expect("OnSetFocus");

            let thread: ITfThreadMgrEventSink = service.cast().expect("ITfThreadMgrEventSink");
            thread
                .OnInitDocumentMgr(None::<&ITfDocumentMgr>)
                .expect("OnInitDocumentMgr");
            thread
                .OnSetFocus(None::<&ITfDocumentMgr>, None::<&ITfDocumentMgr>)
                .expect("OnSetFocus");
            thread
                .OnPushContext(None::<&ITfContext>)
                .expect("OnPushContext");
            thread
                .OnPopContext(None::<&ITfContext>)
                .expect("OnPopContext");
            thread
                .OnUninitDocumentMgr(None::<&ITfDocumentMgr>)
                .expect("OnUninitDocumentMgr");

            let edits: ITfTextEditSink = service.cast().expect("ITfTextEditSink");
            edits
                .OnEndEdit(None::<&ITfContext>, 0, None::<&ITfEditRecord>)
                .expect("OnEndEdit");

            let focus: ITfThreadFocusSink = service.cast().expect("ITfThreadFocusSink");
            focus.OnSetThreadFocus().expect("OnSetThreadFocus");
            focus.OnKillThreadFocus().expect("OnKillThreadFocus");

            let compartment: ITfCompartmentEventSink =
                service.cast().expect("ITfCompartmentEventSink");
            compartment
                .OnChange(&GUID_COMPARTMENT_KEYBOARD_OPENCLOSE)
                .expect("OnChange");
        }
    });
}
