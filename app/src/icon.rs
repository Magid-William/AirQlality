//! The app's own icon on Windows (EXP14): a four-quadrant rectangle.
//!
//! The *design* lives in [`crate::icon_design`], shared with `build.rs`, which
//! bakes the same pixels into the executable's `.ico` resource (so Explorer and
//! the taskbar group show it). Here we materialise those pixels as an `HICON`
//! and hand it to the window with `WM_SETICON`, which governs the title bar and
//! the taskbar button.
//!
//! The tray icon is a separate, verdict-tinted square and is intentionally left
//! untouched (see `tray.rs`).

#![cfg(windows)]

use std::ffi::c_void;

use windows::core::BOOL;
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::Graphics::Gdi::{
    CreateBitmap, CreateDIBSection, DeleteObject, BITMAPINFO, BITMAPINFOHEADER, DIB_RGB_COLORS,
    HGDIOBJ,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateIconIndirect, GetSystemMetrics, SendMessageW, HICON, ICONINFO, ICON_BIG, ICON_SMALL,
    SM_CXICON, SM_CXSMICON, SM_CYICON, SM_CYSMICON, WM_SETICON,
};

use crate::icon_design::app_icon_bgra;

/// Build an `HICON` from top-down 32bpp BGRA pixels of a `size`-wide square.
pub unsafe fn hicon_from_bgra(pixels: &[u8], size: u32) -> HICON {
    // Safety: `pixels` must hold `size * size * 4` bytes (callers guarantee it).
    unsafe {
        let bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: size as i32,
                biHeight: -(size as i32), // negative -> top-down rows
                biPlanes: 1,
                biBitCount: 32,
                biCompression: 0, // BI_RGB
                ..Default::default()
            },
            ..Default::default()
        };
        let mut bits: *mut c_void = std::ptr::null_mut();
        let hbm_color =
            CreateDIBSection(None, &bmi, DIB_RGB_COLORS, &mut bits, None, 0).unwrap_or_default();
        if !bits.is_null() {
            std::ptr::copy_nonoverlapping(pixels.as_ptr(), bits as *mut u8, pixels.len());
        }
        // The mask is required by the API but ignored for 32bpp alpha icons.
        let hbm_mask = CreateBitmap(size as i32, size as i32, 1, 1, None);
        let info = ICONINFO {
            fIcon: BOOL::from(true),
            hbmMask: hbm_mask,
            hbmColor: hbm_color,
            ..Default::default()
        };
        let icon = CreateIconIndirect(&info).unwrap_or_default();
        let _ = DeleteObject(HGDIOBJ(hbm_color.0));
        let _ = DeleteObject(HGDIOBJ(hbm_mask.0));
        icon
    }
}

/// Set `hwnd`'s big and small icons to the app icon, at the DPI-appropriate
/// system sizes. The two `HICON`s are deliberately never destroyed: the window
/// uses them for its whole lifetime (and the OS copies them where needed).
pub fn set_window_icon(hwnd: isize) {
    if hwnd == 0 {
        return;
    }
    unsafe {
        let hwnd = HWND(hwnd as *mut c_void);
        let big = icon_edge(SM_CXICON, SM_CYICON);
        let small = icon_edge(SM_CXSMICON, SM_CYSMICON);
        let hbig = hicon_from_bgra(&app_icon_bgra(big), big);
        let hsmall = hicon_from_bgra(&app_icon_bgra(small), small);
        let _ = SendMessageW(
            hwnd,
            WM_SETICON,
            Some(WPARAM(ICON_BIG as usize)),
            Some(LPARAM(hbig.0 as isize)),
        );
        let _ = SendMessageW(
            hwnd,
            WM_SETICON,
            Some(WPARAM(ICON_SMALL as usize)),
            Some(LPARAM(hsmall.0 as isize)),
        );
    }
}

/// A square edge length from a pair of `SM_*` metrics, clamped to a sane range.
unsafe fn icon_edge(
    cx: windows::Win32::UI::WindowsAndMessaging::SYSTEM_METRICS_INDEX,
    cy: windows::Win32::UI::WindowsAndMessaging::SYSTEM_METRICS_INDEX,
) -> u32 {
    unsafe {
        let w = GetSystemMetrics(cx);
        let h = GetSystemMetrics(cy);
        w.max(h).clamp(16, 256) as u32
    }
}
