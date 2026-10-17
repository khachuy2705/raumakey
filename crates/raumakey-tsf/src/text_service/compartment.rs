//! The keyboard open/close compartment as a number (`VT_I4`).

use core::mem::ManuallyDrop;

use windows::Win32::System::Variant::{
    VARIANT, VARIANT_0, VARIANT_0_0, VARIANT_0_0_0, VT_I4, VariantClear,
};
use windows::Win32::UI::TextServices::ITfCompartment;
use windows::core::Result;

use crate::adapter::mode::open_close_value;

/// The number `compartment` holds, or `None` when it is empty or holds another type.
pub(super) fn read_open_close(compartment: &ITfCompartment) -> Option<i32> {
    // SAFETY: a live compartment of this thread.
    let mut value = unsafe { compartment.GetValue() }.ok()?;
    // SAFETY: `vt` tells which member of the union is valid; `lVal` is read only for
    // `VT_I4`.
    let number = unsafe {
        let inner = &value.Anonymous.Anonymous;
        (inner.vt == VT_I4).then(|| inner.Anonymous.lVal)
    };
    // SAFETY: a VARIANT that GetValue initialized; clearing releases what it owns.
    let _ = unsafe { VariantClear(&mut value) };
    number
}

/// Writes `mode` to `compartment` for the client `client_id` (`VT_I4`, nonzero is open).
pub(super) fn write_open_close(
    compartment: &ITfCompartment,
    client_id: u32,
    mode: bool,
) -> Result<()> {
    let value = VARIANT {
        Anonymous: VARIANT_0 {
            Anonymous: ManuallyDrop::new(VARIANT_0_0 {
                vt: VT_I4,
                wReserved1: 0,
                wReserved2: 0,
                wReserved3: 0,
                Anonymous: VARIANT_0_0_0 {
                    lVal: open_close_value(mode),
                },
            }),
        },
    };
    // SAFETY: a live compartment of this thread and a VT_I4 VARIANT, which owns nothing.
    unsafe { compartment.SetValue(client_id, &value) }
}
