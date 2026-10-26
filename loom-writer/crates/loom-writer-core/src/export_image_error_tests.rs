use crate::{export_pdf, InlineImage, WriterDocument};
use std::io::Cursor;

fn png_with_corrupt_image_data() -> Vec<u8> {
    let pixels = image::RgbaImage::from_pixel(1, 1, image::Rgba([20, 40, 60, 255]));
    let mut bytes = Vec::new();
    pixels
        .write_to(&mut Cursor::new(&mut bytes), image::ImageFormat::Png)
        .expect("encode PNG fixture");

    let idat = bytes
        .windows(4)
        .position(|chunk| chunk == b"IDAT")
        .expect("encoded PNG has image data");
    bytes[idat + 4] ^= 0xff;
    bytes
}

#[test]
fn pdf_export_reports_picture_decode_failure_with_block_context() {
    let image = InlineImage::from_bytes("broken.png", png_with_corrupt_image_data())
        .expect("the PNG header and dimensions are valid");
    assert!(
        image.preview_rgba(1600).is_err(),
        "the fixture's pixel stream must fail during decode"
    );

    let mut document = WriterDocument::new("broken-image", "Broken image");
    document
        .insert_image_block(0, image)
        .expect("insert picture block");
    let Err(error) = export_pdf(&document) else {
        panic!("PDF export silently omitted a picture whose bytes cannot be decoded");
    };
    let message = error.to_string();
    assert!(message.contains("PDF export image block"), "{message}");
    assert!(message.contains("broken.png"), "{message}");
    assert!(message.contains("could not decode picture"), "{message}");
}
