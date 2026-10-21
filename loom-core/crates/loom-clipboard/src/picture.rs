//! Picture bytes from clipboard image data. A copied screenshot or picture
//! arrives as a Windows device-independent bitmap (`CF_DIB`); it is converted to
//! PNG here so every application receives the same format.

/// Largest device-independent bitmap read from the clipboard (a 4K screenshot
/// is about 33 MB). Larger clipboard images are ignored rather than decoded.
pub const MAX_DIB_BYTES: usize = 128 * 1024 * 1024;

/// Converts a `CF_DIB` payload (a `BITMAPINFO` without the file header) into
/// PNG bytes. `None` when the data is not a bitmap this converter understands.
pub fn dib_to_png(dib: &[u8]) -> Option<Vec<u8>> {
    if dib.len() < 40 || dib.len() > MAX_DIB_BYTES {
        return None;
    }
    let header_size = read_u32(dib, 0)? as usize;
    if !(40..=dib.len()).contains(&header_size) {
        return None;
    }
    let bit_count = u16::from_le_bytes(dib.get(14..16)?.try_into().ok()?);
    let compression = read_u32(dib, 16)?;
    let colours_used = read_u32(dib, 32)? as usize;

    // Bit masks follow a plain 40-byte header when compression says they are
    // stored there (BI_BITFIELDS, BI_ALPHABITFIELDS); newer headers hold them.
    let mask_bytes = match (header_size, compression) {
        (40, 3) => 12,
        (40, 6) => 16,
        _ => 0,
    };
    let palette_entries = if colours_used == 0 && bit_count <= 8 {
        1usize << bit_count
    } else {
        colours_used
    };
    let pixel_offset = 14usize
        .checked_add(header_size)?
        .checked_add(mask_bytes)?
        .checked_add(palette_entries.checked_mul(4)?)?;

    // The bitmap file header that image decoders expect in front of the DIB.
    let file_size = u32::try_from(14 + dib.len()).ok()?;
    let mut bmp = Vec::with_capacity(14 + dib.len());
    bmp.extend_from_slice(b"BM");
    bmp.extend_from_slice(&file_size.to_le_bytes());
    bmp.extend_from_slice(&[0, 0, 0, 0]);
    bmp.extend_from_slice(&u32::try_from(pixel_offset).ok()?.to_le_bytes());
    bmp.extend_from_slice(dib);

    let decoded = image::load_from_memory_with_format(&bmp, image::ImageFormat::Bmp).ok()?;
    let mut png = Vec::new();
    decoded
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .ok()?;
    Some(png)
}

fn read_u32(bytes: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(bytes.get(at..at + 4)?.try_into().ok()?))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 2 x 2, 24-bit, bottom-up bitmap as a clipboard carries it: a 40-byte
    /// header followed by rows padded to four bytes.
    fn two_by_two_dib() -> Vec<u8> {
        let mut dib = Vec::new();
        dib.extend_from_slice(&40u32.to_le_bytes()); // biSize
        dib.extend_from_slice(&2i32.to_le_bytes()); // biWidth
        dib.extend_from_slice(&2i32.to_le_bytes()); // biHeight (bottom-up)
        dib.extend_from_slice(&1u16.to_le_bytes()); // biPlanes
        dib.extend_from_slice(&24u16.to_le_bytes()); // biBitCount
        dib.extend_from_slice(&0u32.to_le_bytes()); // biCompression = BI_RGB
        dib.extend_from_slice(&[0u8; 20]); // size, resolution, colours
        let row = [0, 0, 255, 0, 255, 0, 0, 0]; // blue, green, then padding
        dib.extend_from_slice(&row);
        dib.extend_from_slice(&row);
        dib
    }

    #[test]
    fn a_24_bit_dib_becomes_png_bytes_of_the_same_size() {
        let png = dib_to_png(&two_by_two_dib()).expect("the bitmap converts");
        assert_eq!(&png[..4], &[0x89, b'P', b'N', b'G'], "PNG signature");
        let decoded = image::load_from_memory_with_format(&png, image::ImageFormat::Png)
            .expect("the PNG decodes");
        assert_eq!((decoded.width(), decoded.height()), (2, 2));
    }

    #[test]
    fn data_that_is_not_a_bitmap_or_is_truncated_is_refused() {
        assert_eq!(dib_to_png(b"not a bitmap at all"), None);
        let mut short = two_by_two_dib();
        short.truncate(50);
        assert_eq!(dib_to_png(&short), None, "pixel data is missing");
    }
}
