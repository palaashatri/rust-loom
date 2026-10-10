//! The Inter faces that `loom-ui` ships (SIL OFL 1.1, see
//! `loom-ui/ui/fonts/Inter-LICENSE.txt`).
//!
//! The bytes are the same files the Slint shell registers, so a measurement
//! made here describes the glyphs the page actually draws. They are included
//! by path rather than copied so the repository holds one copy. The faces a
//! document is set in (`bundled-inter-text`) and the two intermediate weights
//! (`bundled-inter-weights`) are separate features, so a binary pays for the
//! faces it uses.

/// One bundled face: its file name and bytes.
pub type BundledFace = (&'static str, &'static [u8]);

const FACES: &[BundledFace] = &[
    #[cfg(feature = "bundled-inter-text")]
    (
        "Inter-Regular.ttf",
        include_bytes!("../../loom-ui/ui/fonts/Inter-Regular.ttf"),
    ),
    #[cfg(feature = "bundled-inter-text")]
    (
        "Inter-Italic.ttf",
        include_bytes!("../../loom-ui/ui/fonts/Inter-Italic.ttf"),
    ),
    #[cfg(feature = "bundled-inter-weights")]
    (
        "Inter-Medium.ttf",
        include_bytes!("../../loom-ui/ui/fonts/Inter-Medium.ttf"),
    ),
    #[cfg(feature = "bundled-inter-weights")]
    (
        "Inter-SemiBold.ttf",
        include_bytes!("../../loom-ui/ui/fonts/Inter-SemiBold.ttf"),
    ),
    #[cfg(feature = "bundled-inter-text")]
    (
        "Inter-Bold.ttf",
        include_bytes!("../../loom-ui/ui/fonts/Inter-Bold.ttf"),
    ),
    #[cfg(feature = "bundled-inter-text")]
    (
        "Inter-BoldItalic.ttf",
        include_bytes!("../../loom-ui/ui/fonts/Inter-BoldItalic.ttf"),
    ),
];

/// Every bundled face the enabled features select: six with `bundled-inter`,
/// four with `bundled-inter-text` alone, none with neither.
pub fn bundled_faces() -> &'static [BundledFace] {
    FACES
}

pub(crate) fn bundled_bytes(file: &str) -> Option<&'static [u8]> {
    FACES
        .iter()
        .find(|(name, _)| *name == file)
        .map(|(_, bytes)| *bytes)
}
