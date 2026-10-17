//! Reading the input scope of a context (`GUID_PROP_INPUTSCOPE`, matrix #29).
//!
//! A read-only edit session reads the application property `GUID_PROP_INPUTSCOPE` at the
//! selection: an `ITfInputScope` whose `GetInputScopes` lists the scopes of the field. When
//! the session runs, its answer is delivered: secure when a scope is a password scope
//! (`adapter::scope`), and not secure when the field declares no scope or the property cannot
//! be read. Until a session ran, the document counts as secure
//! (`adapter::documents::ContextInfo::is_secure_field`).

use raumakey_windows::com_server::ObjectGuard;
use windows::Win32::System::Com::CoTaskMemFree;
use windows::Win32::System::Variant::{VARIANT, VT_UNKNOWN, VariantClear};
use windows::Win32::UI::TextServices::{
    GUID_PROP_INPUTSCOPE, ITfContext, ITfEditSession, ITfEditSession_Impl, ITfInputScope,
    TF_CONTEXT_EDIT_CONTEXT_FLAGS, TF_ES_READ,
};
use windows::core::{ComObject, Interface, Result, implement};

use super::context::selection_range;
use super::guarded;
use crate::adapter::scope::is_secure;

/// Most input scopes read from one field.
const MAX_SCOPES: usize = 64;

/// Requests a read-only edit session of `context` (with `flags`, such as `TF_ES_SYNC` or
/// `TF_ES_ASYNCDONTCARE`) that reads its input scope and calls `deliver` with whether the
/// field is a secure field. When TSF refuses the session, `deliver` is never called.
pub(super) fn request(
    context: &ITfContext,
    client_id: u32,
    flags: TF_CONTEXT_EDIT_CONTEXT_FLAGS,
    deliver: Box<dyn Fn(bool)>,
) {
    let session: ITfEditSession = ComObject::new(ScopeSession {
        context: context.clone(),
        deliver,
        _counted: ObjectGuard::new(),
    })
    .to_interface();
    // SAFETY: a live context and edit session of this thread, and the client id TSF gave
    // the activation. A refused request leaves the scope unknown.
    let _ = unsafe { context.RequestEditSession(client_id, &session, TF_ES_READ | flags) };
}

/// The read-only edit session that reads an input scope.
#[implement(ITfEditSession, Agile = false)]
struct ScopeSession {
    context: ITfContext,
    deliver: Box<dyn Fn(bool)>,
    _counted: ObjectGuard,
}

impl ITfEditSession_Impl for ScopeSession_Impl {
    fn DoEditSession(&self, ec: u32) -> Result<()> {
        guarded(|| {
            (self.deliver)(read_secure(&self.context, ec));
            Ok(())
        })
    }
}

/// Whether the field of `context` is a secure field, read in a session with cookie `ec`.
fn read_secure(context: &ITfContext, ec: u32) -> bool {
    // SAFETY: a live context and a valid cookie of a read session.
    let Ok(property) = (unsafe { context.GetAppProperty(&GUID_PROP_INPUTSCOPE) }) else {
        return false;
    };
    // SAFETY: as above.
    let Some(range) = selection_range(context, ec).or_else(|| unsafe { context.GetStart(ec) }.ok())
    else {
        return false;
    };
    // SAFETY: a property and a range of this context, and the session's cookie.
    let Ok(mut value) = (unsafe { property.GetValue(ec, &range) }) else {
        return false;
    };
    let scopes = input_scopes(&value);
    // SAFETY: a VARIANT that GetValue initialized; clearing releases what it owns.
    let _ = unsafe { VariantClear(&mut value) };
    is_secure(&scopes)
}

/// The input scopes of an `ITfInputScope` held by `value` (`VT_UNKNOWN`); none for any other
/// value. At most [`MAX_SCOPES`] are read.
fn input_scopes(value: &VARIANT) -> Vec<i32> {
    // SAFETY: `vt` tells which member of the union is valid; `punkVal` is read only for
    // `VT_UNKNOWN`, and borrowed, not taken, so the VARIANT still owns it.
    let unknown = unsafe {
        let inner = &value.Anonymous.Anonymous;
        if inner.vt != VT_UNKNOWN {
            return Vec::new();
        }
        (*inner.Anonymous.punkVal).clone()
    };
    let Some(scope) = unknown.and_then(|unknown| unknown.cast::<ITfInputScope>().ok()) else {
        return Vec::new();
    };
    let mut array = core::ptr::null_mut();
    let mut count = 0_u32;
    // SAFETY: a live input scope and writable out parameters.
    if unsafe { scope.GetInputScopes(&mut array, &mut count) }.is_err() || array.is_null() {
        return Vec::new();
    }
    let read = usize::try_from(count).unwrap_or(0).min(MAX_SCOPES);
    // SAFETY: GetInputScopes returned an array of `count` scopes, of which the first `read`
    // are copied.
    let scopes = unsafe { core::slice::from_raw_parts(array, read) }
        .iter()
        .map(|scope| scope.0)
        .collect();
    // SAFETY: the array was allocated with CoTaskMemAlloc for the caller, who frees it once.
    unsafe { CoTaskMemFree(Some(array.cast_const().cast())) };
    scopes
}

#[cfg(test)]
mod tests {
    use core::mem::ManuallyDrop;

    use windows::Win32::Foundation::E_NOTIMPL;
    use windows::Win32::System::Com::{CoTaskMemAlloc, IAgileObject};
    use windows::Win32::System::Variant::{
        VARIANT, VARIANT_0, VARIANT_0_0, VARIANT_0_0_0, VT_I4, VT_UNKNOWN, VariantClear,
    };
    use windows::Win32::UI::TextServices::{ITfInputScope, ITfInputScope_Impl, InputScope};
    use windows::core::{BSTR, Error, IUnknown, Result, implement};

    use super::input_scopes;

    /// An input scope of a field, as an application implements it.
    #[implement(ITfInputScope)]
    struct Scopes(Vec<i32>);

    impl ITfInputScope_Impl for Scopes_Impl {
        fn GetInputScopes(&self, scopes: *mut *mut InputScope, count: *mut u32) -> Result<()> {
            let bytes = self.0.len() * size_of::<InputScope>();
            // SAFETY: a plain allocation that the caller frees with CoTaskMemFree.
            let array = unsafe { CoTaskMemAlloc(bytes) }.cast::<InputScope>();
            assert!(!array.is_null(), "allocated");
            for (index, scope) in self.0.iter().enumerate() {
                // SAFETY: the allocation holds `len` scopes.
                unsafe { array.add(index).write(InputScope(*scope)) };
            }
            // SAFETY: the caller's out parameters.
            unsafe {
                *scopes = array;
                *count = u32::try_from(self.0.len()).expect("few scopes");
            }
            Ok(())
        }

        fn GetPhrase(&self, _phrases: *mut *mut BSTR, _count: *mut u32) -> Result<()> {
            Err(Error::from_hresult(E_NOTIMPL))
        }

        fn GetRegularExpression(&self) -> Result<BSTR> {
            Err(Error::from_hresult(E_NOTIMPL))
        }

        fn GetSRGS(&self) -> Result<BSTR> {
            Err(Error::from_hresult(E_NOTIMPL))
        }

        fn GetXML(&self) -> Result<BSTR> {
            Err(Error::from_hresult(E_NOTIMPL))
        }
    }

    /// A VARIANT that holds `unknown` (`VT_UNKNOWN`), as `GetValue` returns it.
    fn holding(unknown: IUnknown) -> VARIANT {
        let mut value = VARIANT::default();
        value.Anonymous = VARIANT_0 {
            Anonymous: ManuallyDrop::new(VARIANT_0_0 {
                vt: VT_UNKNOWN,
                wReserved1: 0,
                wReserved2: 0,
                wReserved3: 0,
                Anonymous: VARIANT_0_0_0 {
                    punkVal: ManuallyDrop::new(Some(unknown)),
                },
            }),
        };
        value
    }

    fn scopes_of(scopes: &[i32]) -> Vec<i32> {
        let scope: ITfInputScope = Scopes(scopes.to_vec()).into();
        let mut value = holding(scope.into());
        let read = input_scopes(&value);
        // SAFETY: a VARIANT built above; clearing releases the interface it holds.
        unsafe { VariantClear(&mut value) }.expect("VariantClear");
        read
    }

    #[test]
    fn the_scopes_of_an_input_scope_object_are_read() {
        assert_eq!(scopes_of(&[31]), [31]);
        assert_eq!(scopes_of(&[0, 1, 64]), [0, 1, 64]);
        assert!(scopes_of(&[]).is_empty());
    }

    #[test]
    fn a_long_list_is_read_up_to_the_limit() {
        let many: Vec<i32> = (0..100).collect();
        assert_eq!(scopes_of(&many), many[..super::MAX_SCOPES]);
    }

    #[test]
    fn values_without_an_input_scope_object_have_no_scopes() {
        assert!(input_scopes(&VARIANT::default()).is_empty(), "VT_EMPTY");
        let mut number = VARIANT::default();
        number.Anonymous = VARIANT_0 {
            Anonymous: ManuallyDrop::new(VARIANT_0_0 {
                vt: VT_I4,
                wReserved1: 0,
                wReserved2: 0,
                wReserved3: 0,
                Anonymous: VARIANT_0_0_0 { lVal: 31 },
            }),
        };
        assert!(input_scopes(&number).is_empty(), "VT_I4");
        let mut unrelated = holding(Unrelated.into());
        assert!(input_scopes(&unrelated).is_empty(), "not an ITfInputScope");
        // SAFETY: a VARIANT built above; clearing releases the interface it holds.
        unsafe { VariantClear(&mut unrelated) }.expect("VariantClear");
    }

    /// A COM object that is no `ITfInputScope`.
    #[implement(IAgileObject)]
    struct Unrelated;

    impl windows::Win32::System::Com::IAgileObject_Impl for Unrelated_Impl {}
}
