//! Package-owned image assets for Sheets workbooks.
//!
//! `.loomtable` keeps the original path as a useful fallback, but saves a
//! copy of every image inside the package so a workbook remains portable.

use std::collections::BTreeMap;
use std::path::Path;

use loom_package::PackageArchive;
use loom_sheets_core::{Sheet, SheetObjectKind, WorkbookFile};

pub(crate) struct EmbeddedAsset {
    pub(crate) path: String,
    pub(crate) mime: &'static str,
    pub(crate) bytes: Vec<u8>,
}

pub(crate) fn prepare_workbook(
    sheets: &[Sheet],
) -> Result<(Vec<Sheet>, Vec<EmbeddedAsset>), String> {
    let mut persisted = sheets.to_vec();
    let mut assets = Vec::new();
    let mut asset_paths = BTreeMap::<[u8; 32], String>::new();
    for (sheet_index, sheet) in persisted.iter_mut().enumerate() {
        for (object_index, object) in sheet.objects.iter_mut().enumerate() {
            if object.kind != SheetObjectKind::Image {
                continue;
            }
            let bytes = object.embedded.clone().or_else(|| {
                (!object.path.trim().is_empty())
                    .then(|| std::fs::read(&object.path).ok())
                    .flatten()
            });
            let Some(bytes) = bytes else {
                return Err(format!("image source is unavailable: {}", object.path));
            };
            let digest = loom_package::zip::sha256(&bytes);
            if let Some(asset_path) = asset_paths.get(&digest) {
                object.asset = Some(asset_path.clone());
                object.embedded = Some(bytes);
                continue;
            }
            let extension = image_extension(&object.path);
            let asset_path =
                format!("content/assets/image-{sheet_index}-{object_index}.{extension}");
            object.asset = Some(asset_path.clone());
            object.embedded = Some(bytes.clone());
            asset_paths.insert(digest, asset_path.clone());
            assets.push(EmbeddedAsset {
                path: asset_path,
                mime: image_mime(extension),
                bytes,
            });
        }
    }
    Ok((persisted, assets))
}

pub(crate) fn attach_workbook_assets(
    workbook: &mut WorkbookFile,
    archive: &PackageArchive,
) -> Result<(), String> {
    for sheet in &mut workbook.sheets {
        for object in &mut sheet.objects {
            let Some(asset_path) = object.asset.as_deref() else {
                continue;
            };
            if !asset_path.starts_with("content/assets/") {
                return Err(format!("invalid image asset path: {asset_path}"));
            }
            let bytes = archive
                .get(asset_path)
                .ok_or_else(|| format!("missing image asset {asset_path}"))?;
            object.embedded = Some(bytes.to_vec());
        }
    }
    Ok(())
}

fn image_extension(path: &str) -> &'static str {
    match Path::new(path)
        .extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("jpg") | Some("jpeg") => "jpg",
        Some("webp") => "webp",
        Some("gif") => "gif",
        Some("svg") => "svg",
        _ => "png",
    }
}

fn image_mime(extension: &str) -> &'static str {
    match extension {
        "jpg" => "image/jpeg",
        "webp" => "image/webp",
        "gif" => "image/gif",
        "svg" => "image/svg+xml",
        _ => "image/png",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use loom_sheets_core::{CellRef, SheetObject};

    #[test]
    fn image_asset_names_are_safe_and_deterministic() {
        let mut sheet = Sheet::new("Assets");
        let mut image =
            SheetObject::image(CellRef { row: 0, col: 0 }, "/tmp/hero.jpg").expect("image object");
        image.embedded = Some(vec![1, 2, 3]);
        sheet.objects.push(image);

        let (sheets, assets) = prepare_workbook(&[sheet]).expect("prepare assets");
        assert_eq!(assets[0].path, "content/assets/image-0-0.jpg");
        assert_eq!(assets[0].mime, "image/jpeg");
        assert_eq!(assets[0].bytes, vec![1, 2, 3]);
        assert_eq!(
            sheets[0].objects[0].asset.as_deref(),
            Some("content/assets/image-0-0.jpg")
        );
    }

    #[test]
    fn identical_image_bytes_share_one_package_asset() {
        let mut sheet = Sheet::new("Assets");
        for (row, path) in [(0, "/tmp/hero.png"), (1, "/tmp/hero-copy.png")] {
            let mut image =
                SheetObject::image(CellRef { row, col: 0 }, path).expect("image object");
            image.embedded = Some(vec![137, 80, 78, 71]);
            sheet.objects.push(image);
        }

        let (sheets, assets) = prepare_workbook(&[sheet]).expect("prepare assets");
        assert_eq!(assets.len(), 1);
        assert_eq!(
            sheets[0].objects[0].asset, sheets[0].objects[1].asset,
            "duplicate payloads must reference one package entry"
        );
    }
}
