//! Pure, dependency-free design of the app icon (EXP14): a square split into
//! four flush `size/2` quadrants in traffic-light order.
//!
//! This file is shared *verbatim* by the app (which turns the pixels into an
//! `HICON` and calls `WM_SETICON`) and by `build.rs` (which encodes the same
//! pixels into the `.ico` linked into the executable). Because both include the
//! same source, the window icon and the Explorer icon can never drift apart.
//!
//! ```
//! +---------+---------+
//! |   red   | yellow  |  red    = Keep closed
//! +---------+---------+  yellow = Vent
//! |  green  |  gray   |  green  = Open
//! +---------+---------+  gray   = unknown / loading
//! ```
#![allow(dead_code)] // `ICO_SIZES` / `encode_ico` are only used by `build.rs`.

use crate::theme;

/// The pixel sizes baked into the embedded `.ico`, smallest (title bar / list)
/// to largest (Explorer's extra-large view). 256 is a valid ICO width; the ICO
/// container stores it as `0`.
pub const ICO_SIZES: [u32; 4] = [16, 32, 48, 256];

/// The `0xRRGGBB` colour of the quadrant containing pixel `(x, y)` of a
/// `size`-wide square. Left/top win for odd sizes (`half = size / 2`).
pub fn quadrant(size: u32, x: u32, y: u32) -> u32 {
    let half = size / 2;
    match (x < half, y < half) {
        (true, true) => theme::CLOSED,  // red,    top-left
        (false, true) => theme::VENT,   // yellow, top-right
        (true, false) => theme::OPEN,   // green,  bottom-left
        (false, false) => theme::MUTED, // gray,   bottom-right
    }
}

/// `size * size` 32bpp **top-down BGRA** pixels for the app icon, the layout
/// both a `BITMAPINFOHEADER` DIB and our `.ico` encoder expect.
pub fn app_icon_bgra(size: u32) -> Vec<u8> {
    let size = size.max(1);
    let mut out = Vec::with_capacity((size * size * 4) as usize);
    for y in 0..size {
        for x in 0..size {
            out.extend_from_slice(&bgra(quadrant(size, x, y)));
        }
    }
    out
}

/// `0xRRGGBB` -> `[B, G, R, 0xff]`, the byte order a 32bpp DIB / ICO expects.
pub fn bgra(color: u32) -> [u8; 4] {
    [
        (color & 0xff) as u8,
        ((color >> 8) & 0xff) as u8,
        ((color >> 16) & 0xff) as u8,
        0xff,
    ]
}

/// Encode the icon at each `sizes` entry as a BMP-flavour `.ico` byte stream.
///
/// BMP (not PNG) entries keep the encoder pure `std`, so `build.rs` needs no
/// image crate; every Windows version reads them. Each image is an uncompressed
/// 32bpp `BITMAPINFOHEADER` DIB (colour rows bottom-up) followed by the 1bpp
/// AND mask, which is all zero because every pixel is opaque.
pub fn encode_ico(sizes: &[u32]) -> Vec<u8> {
    let mut out = Vec::new();
    // ICONDIR: reserved(0), type(1 = icon), count.
    out.extend_from_slice(&0u16.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&(sizes.len() as u16).to_le_bytes());

    // ICONDIRENTRY for each image; the data offsets follow the full directory.
    let mut offset = (6 + sizes.len() * 16) as u32;
    for &size in sizes {
        let dim = if size >= 256 { 0u8 } else { size as u8 };
        let bytes = dib_len(size) as u32;
        out.push(dim); // width (0 = 256)
        out.push(dim); // height (0 = 256)
        out.push(0); // palette size (0 = no palette)
        out.push(0); // reserved
        out.extend_from_slice(&1u16.to_le_bytes()); // colour planes
        out.extend_from_slice(&32u16.to_le_bytes()); // bits per pixel
        out.extend_from_slice(&bytes.to_le_bytes());
        out.extend_from_slice(&offset.to_le_bytes());
        offset += bytes;
    }

    for &size in sizes {
        write_dib(&mut out, size);
    }
    out
}

/// Byte length of one BMP image entry: header + colour pixels + AND mask.
fn dib_len(size: u32) -> usize {
    40 + (size * size * 4) as usize + mask_stride(size) as usize * size as usize
}

/// AND-mask row stride: 1bpp, padded to a 4-byte boundary.
fn mask_stride(size: u32) -> u32 {
    size.div_ceil(32) * 4
}

/// Append one uncompressed 32bpp DIB image (see [`encode_ico`]).
fn write_dib(out: &mut Vec<u8>, size: u32) {
    let mask = mask_stride(size);
    let image = size * size * 4 + mask * size;
    // BITMAPINFOHEADER.
    out.extend_from_slice(&40u32.to_le_bytes()); // biSize
    out.extend_from_slice(&(size as i32).to_le_bytes()); // biWidth
    out.extend_from_slice(&((size * 2) as i32).to_le_bytes()); // biHeight = colour + mask
    out.extend_from_slice(&1u16.to_le_bytes()); // biPlanes
    out.extend_from_slice(&32u16.to_le_bytes()); // biBitCount
    out.extend_from_slice(&0u32.to_le_bytes()); // biCompression = BI_RGB
    out.extend_from_slice(&image.to_le_bytes()); // biSizeImage
    out.extend_from_slice(&[0u8; 16]); // biXPelsPerMeter, biYPelsPerMeter, biClrUsed, biClrImportant

    // Colour pixels, bottom-up (the DIB convention).
    for y in (0..size).rev() {
        for x in 0..size {
            out.extend_from_slice(&bgra(quadrant(size, x, y)));
        }
    }
    // AND mask: zero = fully opaque for every pixel.
    out.resize(out.len() + (mask * size) as usize, 0);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quadrants_are_traffic_light_colours() {
        // size 8 -> half 4. Centre points of each quadrant.
        assert_eq!(quadrant(8, 1, 1), theme::CLOSED); // red,    top-left
        assert_eq!(quadrant(8, 6, 1), theme::VENT); // yellow, top-right
        assert_eq!(quadrant(8, 1, 6), theme::OPEN); // green,  bottom-left
        assert_eq!(quadrant(8, 6, 6), theme::MUTED); // gray,   bottom-right
    }

    #[test]
    fn bgra_is_the_dib_byte_order() {
        // 0xRRGGBB -> [B, G, R, 0xff].
        assert_eq!(bgra(0x3fb950), [0x50, 0xb9, 0x3f, 0xff]);
    }

    #[test]
    fn app_icon_pixels_are_the_four_quadrants() {
        let size = 4u32; // half = 2
        let px = app_icon_bgra(size);
        assert_eq!(px.len(), (size * size * 4) as usize);
        let at = |x: u32, y: u32| {
            let i = ((y * size + x) * 4) as usize;
            [px[i], px[i + 1], px[i + 2], px[i + 3]]
        };
        assert_eq!(at(0, 0), bgra(theme::CLOSED));
        assert_eq!(at(3, 0), bgra(theme::VENT));
        assert_eq!(at(0, 3), bgra(theme::OPEN));
        assert_eq!(at(3, 3), bgra(theme::MUTED));
        // Flush: no gap - two adjacent pixels straddling the seam differ.
        assert_ne!(at(1, 1), at(2, 1));
    }

    #[test]
    fn a_zero_size_still_yields_one_pixel() {
        assert_eq!(app_icon_bgra(0).len(), 4);
    }

    #[test]
    fn ico_header_describes_every_image() {
        let sizes = [16u32, 256u32];
        let ico = encode_ico(&sizes);
        // ICONDIR.
        assert_eq!(&ico[0..2], &0u16.to_le_bytes()); // reserved
        assert_eq!(&ico[2..4], &1u16.to_le_bytes()); // icon type
        assert_eq!(&ico[4..6], &2u16.to_le_bytes()); // two images

        // First ICONDIRENTRY: 16x16, 32bpp, offset right after the directory.
        assert_eq!(ico[6], 16);
        assert_eq!(ico[7], 16);
        assert_eq!(&ico[12..14], &32u16.to_le_bytes());
        let first_off = u32::from_le_bytes(ico[18..22].try_into().unwrap());
        assert_eq!(first_off as usize, 6 + sizes.len() * 16);
        // Second entry encodes 256 as 0.
        assert_eq!(ico[6 + 16], 0);
        assert_eq!(ico[7 + 16], 0);
        // Total length matches the two DIB payloads.
        assert_eq!(
            ico.len(),
            6 + sizes.len() * 16 + sizes.iter().map(|&s| dib_len(s)).sum::<usize>()
        );
    }
}
