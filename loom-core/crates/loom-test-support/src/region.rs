//! Pixel checks on one rectangle of a capture.
//!
//! A text change should reach some parts of a window and not others. These
//! helpers compare one window-pixel rectangle of two captures of the same
//! window, so a test can show that window chrome changed while document
//! content did not.

use image::RgbaImage;

/// A window-pixel rectangle: left, top, width and height.
pub type PixelRect = (u32, u32, u32, u32);

/// The part of `rect` inside `image` as `(x0, y0, x1, y1)`, exclusive at the end.
fn clipped(image: &RgbaImage, rect: PixelRect) -> (u32, u32, u32, u32) {
    let (x, y, width, height) = rect;
    (
        x.min(image.width()),
        y.min(image.height()),
        x.saturating_add(width).min(image.width()),
        y.saturating_add(height).min(image.height()),
    )
}

/// Whether any pixel inside `rect` differs between two captures.
///
/// Captures of different sizes always count as changed.
pub fn region_changed(before: &RgbaImage, after: &RgbaImage, rect: PixelRect) -> bool {
    if before.dimensions() != after.dimensions() {
        return true;
    }
    let (x0, y0, x1, y1) = clipped(before, rect);
    (y0..y1).any(|y| (x0..x1).any(|x| before.get_pixel(x, y) != after.get_pixel(x, y)))
}

/// Whether `rect` holds at least two colours, so something was drawn in it.
///
/// An empty rectangle, or one outside the image, is never drawn.
pub fn region_is_drawn(image: &RgbaImage, rect: PixelRect) -> bool {
    let (x0, y0, x1, y1) = clipped(image, rect);
    let mut first = None;
    for y in y0..y1 {
        for x in x0..x1 {
            let pixel = image.get_pixel(x, y);
            match first {
                None => first = Some(pixel),
                Some(color) if color != pixel => return true,
                Some(_) => {}
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    fn canvas(color: Rgba<u8>) -> RgbaImage {
        RgbaImage::from_pixel(8, 6, color)
    }

    #[test]
    fn identical_captures_do_not_change_any_region() {
        let a = canvas(Rgba([10, 20, 30, 255]));
        assert!(!region_changed(&a, &a.clone(), (0, 0, 8, 6)));
    }

    #[test]
    fn one_changed_pixel_inside_the_region_is_a_change() {
        let before = canvas(Rgba([0, 0, 0, 255]));
        let mut after = before.clone();
        after.put_pixel(3, 2, Rgba([255, 255, 255, 255]));
        assert!(region_changed(&before, &after, (2, 1, 3, 3)));
        assert!(!region_changed(&before, &after, (5, 0, 3, 6)));
    }

    #[test]
    fn captures_of_different_sizes_count_as_changed() {
        let small = RgbaImage::new(4, 4);
        let large = RgbaImage::new(8, 8);
        assert!(region_changed(&small, &large, (0, 0, 1, 1)));
    }

    #[test]
    fn a_flat_region_is_not_drawn_and_a_region_with_two_colours_is() {
        let mut image = canvas(Rgba([255, 255, 255, 255]));
        assert!(!region_is_drawn(&image, (0, 0, 8, 6)));
        image.put_pixel(1, 1, Rgba([0, 0, 0, 255]));
        assert!(region_is_drawn(&image, (0, 0, 8, 6)));
        assert!(!region_is_drawn(&image, (2, 2, 2, 2)));
        assert!(!region_is_drawn(&image, (100, 100, 4, 4)));
    }
}
