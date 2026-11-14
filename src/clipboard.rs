//! Windows clipboard: image (CF_DIB + registered "PNG") and text.
//!
//! Under Wine/Proton the X11 driver bridges these to the Linux clipboard
//! (`image/png`, `image/bmp`, UTF-8 text), so a paste into Discord just works.

use std::{ptr, thread, time::Duration};

use windows_sys::Win32::{
    Foundation::{GlobalFree, HANDLE},
    Graphics::Gdi::{BI_RGB, BITMAPINFOHEADER},
    System::{
        DataExchange::{
            CloseClipboard, EmptyClipboard, OpenClipboard, RegisterClipboardFormatW,
            SetClipboardData,
        },
        Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalUnlock},
        Ole::{CF_DIB, CF_UNICODETEXT},
    },
};

struct Open;

impl Open {
    fn new() -> Result<Self, String> {
        // Another app may hold the clipboard briefly; retry a few times.
        for _ in 0..10 {
            if unsafe { OpenClipboard(ptr::null_mut()) } != 0 {
                if unsafe { EmptyClipboard() } == 0 {
                    unsafe { CloseClipboard() };
                    return Err("EmptyClipboard failed".into());
                }
                return Ok(Open);
            }
            thread::sleep(Duration::from_millis(20));
        }
        Err("clipboard is busy".into())
    }

    fn put(&self, format: u32, bytes: &[u8]) -> Result<(), String> {
        unsafe {
            let mem = GlobalAlloc(GMEM_MOVEABLE, bytes.len());
            if mem.is_null() {
                return Err("GlobalAlloc failed".into());
            }
            let dst = GlobalLock(mem) as *mut u8;
            if dst.is_null() {
                GlobalFree(mem);
                return Err("GlobalLock failed".into());
            }
            ptr::copy_nonoverlapping(bytes.as_ptr(), dst, bytes.len());
            GlobalUnlock(mem);
            // On success the clipboard owns `mem`.
            if SetClipboardData(format, mem as HANDLE).is_null() {
                GlobalFree(mem);
                return Err(format!("SetClipboardData({format}) failed"));
            }
        }
        Ok(())
    }
}

impl Drop for Open {
    fn drop(&mut self) {
        unsafe { CloseClipboard() };
    }
}

pub fn set_image(width: u32, height: u32, rgba: &[u8], png: &[u8], matte: [u8; 3]) -> Result<(), String> {
    let dib = dib(width, height, rgba, matte);
    let png_format = unsafe { RegisterClipboardFormatW(crate::wide("PNG").as_ptr()) };
    let clip = Open::new()?;
    // PNG first: apps that understand it prefer it (keeps transparency).
    if png_format != 0 && !png.is_empty() {
        clip.put(png_format, png)?;
    }
    clip.put(CF_DIB as u32, &dib)
}

pub fn set_text(text: &str) -> Result<(), String> {
    // Windows expects CRLF line endings.
    let utf16 = crate::wide(&text.replace('\n', "\r\n"));
    let bytes: Vec<u8> = utf16.iter().flat_map(|c| c.to_le_bytes()).collect();
    Open::new()?.put(CF_UNICODETEXT as u32, &bytes)
}

/// Bottom-up 32-bit BGRA DIB; transparent pixels are blended onto `matte`
/// because most DIB consumers ignore alpha.
fn dib(width: u32, height: u32, rgba: &[u8], matte: [u8; 3]) -> Vec<u8> {
    let header = BITMAPINFOHEADER {
        biSize: size_of::<BITMAPINFOHEADER>() as u32,
        biWidth: width as i32,
        biHeight: height as i32,
        biPlanes: 1,
        biBitCount: 32,
        biCompression: BI_RGB,
        biSizeImage: width * height * 4,
        biXPelsPerMeter: 0,
        biYPelsPerMeter: 0,
        biClrUsed: 0,
        biClrImportant: 0,
    };
    let mut out = Vec::with_capacity(size_of::<BITMAPINFOHEADER>() + rgba.len());
    let hb: [u8; size_of::<BITMAPINFOHEADER>()] = unsafe { std::mem::transmute(header) };
    out.extend_from_slice(&hb);
    let stride = width as usize * 4;
    for row in (0..height as usize).rev() {
        for px in rgba[row * stride..(row + 1) * stride].chunks_exact(4) {
            let a = px[3] as u32;
            let mix = |c: u8, m: u8| ((c as u32 * a + m as u32 * (255 - a)) / 255) as u8;
            out.extend_from_slice(&[mix(px[2], matte[2]), mix(px[1], matte[1]), mix(px[0], matte[0]), 255]);
        }
    }
    out
}
