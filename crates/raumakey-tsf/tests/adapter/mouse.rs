//! Mouse messages that are button presses (matrix #19: a click ends the word).

use raumakey_tip::adapter::mouse::is_button_press;

#[test]
fn every_button_press_and_double_click_counts() {
    let presses = [
        (0x0201, "WM_LBUTTONDOWN"),
        (0x0203, "WM_LBUTTONDBLCLK"),
        (0x0204, "WM_RBUTTONDOWN"),
        (0x0206, "WM_RBUTTONDBLCLK"),
        (0x0207, "WM_MBUTTONDOWN"),
        (0x0209, "WM_MBUTTONDBLCLK"),
        (0x020B, "WM_XBUTTONDOWN"),
        (0x020D, "WM_XBUTTONDBLCLK"),
        (0x00A1, "WM_NCLBUTTONDOWN"),
        (0x00A3, "WM_NCLBUTTONDBLCLK"),
        (0x00A4, "WM_NCRBUTTONDOWN"),
        (0x00A6, "WM_NCRBUTTONDBLCLK"),
        (0x00A7, "WM_NCMBUTTONDOWN"),
        (0x00A9, "WM_NCMBUTTONDBLCLK"),
        (0x00AB, "WM_NCXBUTTONDOWN"),
        (0x00AD, "WM_NCXBUTTONDBLCLK"),
    ];
    for (message, name) in presses {
        assert!(is_button_press(message), "{name}");
    }
}

#[test]
fn releases_moves_and_wheels_do_not_count() {
    let others = [
        (0x0200, "WM_MOUSEMOVE"),
        (0x0202, "WM_LBUTTONUP"),
        (0x0205, "WM_RBUTTONUP"),
        (0x0208, "WM_MBUTTONUP"),
        (0x020C, "WM_XBUTTONUP"),
        (0x020A, "WM_MOUSEWHEEL"),
        (0x020E, "WM_MOUSEHWHEEL"),
        (0x00A0, "WM_NCMOUSEMOVE"),
        (0x00A2, "WM_NCLBUTTONUP"),
        (0x00A5, "WM_NCRBUTTONUP"),
        (0x00A8, "WM_NCMBUTTONUP"),
        (0x00AC, "WM_NCXBUTTONUP"),
        (0x02A1, "WM_MOUSEHOVER"),
        (0x02A3, "WM_MOUSELEAVE"),
        (0x0100, "WM_KEYDOWN"),
    ];
    for (message, name) in others {
        assert!(!is_button_press(message), "{name}");
    }
}
