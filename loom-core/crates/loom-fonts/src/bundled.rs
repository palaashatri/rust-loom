//! The Inter faces that `loom-ui` ships (SIL OFL 1.1, see
//! `loom-ui/ui/fonts/Inter-LICENSE.txt`).
//!
//! The bytes are the same files the Slint shell registers, so a measurement
//! made here describes the glyphs the page actually draws. They are included
//! by path rather than copied so the repository holds one copy.

/// One bundled face: its file name and bytes.
pub type BundledFace = (&'static str, &'static [u8]);

#[cfg(feature = "bundled-inter")]
const FACES: &[BundledFace] = &[
    (
        "Inter-Regular.ttf",
        include_bytes!("../../loom-ui/ui/fonts/Inter-Regular.ttf"),
    ),
    (
        "Inter-Italic.ttf",
        include_bytes!("../../loom-ui/ui/fonts/Inter-Italic.ttf"),
    ),
    (
        "Inter-Medium.ttf",
        include_bytes!("../../loom-ui/ui/fonts/Inter-Medium.ttf"),
    ),
    (
        "Inter-SemiBold.ttf",
        include_bytes!("../../loom-ui/ui/fonts/Inter-SemiBold.ttf"),
    ),
    (
        "Inter-Bold.ttf",
        include_bytes!("../../loom-ui/ui/fonts/Inter-Bold.ttf"),
    ),
    (
        "Inter-BoldItalic.ttf",
        include_bytes!("../../loom-ui/ui/fonts/Inter-BoldItalic.ttf"),
    ),
];

#[cfg(not(feature = "bundled-inter"))]
const FACES: &[BundledFace] = &[];

/// Every bundled face. Empty when the `bundled-inter` feature is off.
pub fn bundled_faces() -> &'static [BundledFace] {
    FACES
}

pub(crate) fn bundled_bytes(file: &str) -> Option<&'static [u8]> {
    FACES
        .iter()
        .find(|(name, _)| *name == file)
        .map(|(_, bytes)| *bytes)
}
