//! Opening links in the system browser. Under Wine, ShellExecute hands
//! http(s) URLs to `winebrowser`, which calls `xdg-open` on the Linux side.

use windows_sys::Win32::UI::{Shell::ShellExecuteW, WindowsAndMessaging::SW_SHOWNORMAL};

use crate::wide;

pub fn open_url(url: &str) -> Result<(), String> {
    let (verb, target) = (wide("open"), wide(url));
    let r = unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            verb.as_ptr(),
            target.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            SW_SHOWNORMAL,
        )
    };
    // Values <= 32 are errors.
    if r as isize > 32 { Ok(()) } else { Err(format!("ShellExecute error {}", r as isize)) }
}
