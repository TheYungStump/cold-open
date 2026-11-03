//! The window's icon (docs/specs/release.md §3): the packaging's 64-pixel
//! PNG, decoded here with the inflater the app already has, since iced's
//! image support would bring a whole image library for one picture.

/// The app's identifier, which Wayland matches to the desktop entry and
/// its icon.
pub const APP_ID: &str = "io.github.theyungstump.coldopen";

const PNG: &[u8] = include_bytes!("../../../.github/packaging/icon/png/64.png");

/// The window icon, or `None` if the PNG can't be read (the window then
/// has the system's default).
pub fn window_icon() -> Option<iced::window::Icon> {
    let (w, h, rgba) = decode_rgba(PNG)?;
    iced::window::icon::from_rgba(rgba, w, h).ok()
}

/// Decodes an 8-bit RGBA, non-interlaced PNG, as `make_icons.py` writes
/// and as artwork exported for it should be.
pub fn decode_rgba(png: &[u8]) -> Option<(u32, u32, Vec<u8>)> {
    let mut at = png.strip_prefix(b"\x89PNG\r\n\x1a\n".as_slice())?;
    let (mut w, mut h) = (0u32, 0u32);
    let mut idat = Vec::new();
    while at.len() >= 12 {
        let len = u32::from_be_bytes(at[..4].try_into().ok()?) as usize;
        let kind = &at[4..8];
        let data = at.get(8..8 + len)?;
        match kind {
            b"IHDR" => {
                w = u32::from_be_bytes(data.get(..4)?.try_into().ok()?);
                h = u32::from_be_bytes(data.get(4..8)?.try_into().ok()?);
                // 8 bits, colour type 6 (RGBA), no interlace.
                if data.get(8..13)? != [8, 6, 0, 0, 0] {
                    return None;
                }
            }
            b"IDAT" => idat.extend_from_slice(data),
            b"IEND" => break,
            _ => {}
        }
        at = at.get(12 + len..)?;
    }
    let raw = miniz_oxide::inflate::decompress_to_vec_zlib(&idat).ok()?;
    let stride = w as usize * 4;
    let mut out = vec![0u8; stride * h as usize];
    for y in 0..h as usize {
        let line = raw.get(y * (stride + 1)..(y + 1) * (stride + 1))?;
        let (filter, line) = (line[0], &line[1..]);
        for x in 0..stride {
            let a = if x >= 4 { out[y * stride + x - 4] } else { 0 };
            let b = if y > 0 { out[(y - 1) * stride + x] } else { 0 };
            let c = if x >= 4 && y > 0 {
                out[(y - 1) * stride + x - 4]
            } else {
                0
            };
            let predicted = match filter {
                0 => 0,
                1 => a,
                2 => b,
                3 => ((u16::from(a) + u16::from(b)) / 2) as u8,
                4 => paeth(a, b, c),
                _ => return None,
            };
            out[y * stride + x] = line[x].wrapping_add(predicted);
        }
    }
    Some((w, h, out))
}

fn paeth(a: u8, b: u8, c: u8) -> u8 {
    let p = i16::from(a) + i16::from(b) - i16::from(c);
    let (pa, pb, pc) = (
        (p - i16::from(a)).abs(),
        (p - i16::from(b)).abs(),
        (p - i16::from(c)).abs(),
    );
    if pa <= pb && pa <= pc {
        a
    } else if pb <= pc {
        b
    } else {
        c
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_icon_decodes() {
        let (w, h, rgba) = decode_rgba(PNG).expect("the packaging's PNG");
        assert_eq!((w, h), (64, 64));
        assert_eq!(rgba.len(), 64 * 64 * 4);
        // A corner is clear and the middle is not.
        assert_eq!(rgba[3], 0);
        assert_eq!(rgba[(32 * 64 + 32) * 4 + 3], 255);
        assert!(window_icon().is_some());
    }
}
