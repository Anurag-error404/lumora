//! The set of models LUMORA knows how to install.
//!
//! Every entry pins a `sha256` taken from the upstream repository. A downloaded
//! file that does not hash to the pinned value is discarded, so a corrupted or
//! substituted download can never be loaded as a model.

/// What a model does. Also the `kind` column in `ml_models` and `ml_jobs`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelKind {
    /// Encodes an image into the shared CLIP embedding space.
    ClipImage,
    /// Encodes a text query into the same space.
    ClipText,
    /// PP-OCR text detection (DBNet).
    OcrDetect,
    /// PP-OCR text recognition (CRNN/SVTR) or its character dictionary.
    OcrRecognize,
    /// Combined OCR job kind in `ml_jobs` (detect + recognize pipeline).
    Ocr,
    /// SCRFD face detection.
    FaceDetect,
    /// ArcFace face embedding.
    FaceEmbed,
    /// Combined faces job kind in `ml_jobs` (detect + embed + cluster).
    Faces,
}

impl ModelKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ClipImage => "clip_image",
            Self::ClipText => "clip_text",
            Self::OcrDetect => "ocr_detect",
            Self::OcrRecognize => "ocr_recognize",
            Self::Ocr => "ocr",
            Self::FaceDetect => "face_detect",
            Self::FaceEmbed => "face_embed",
            Self::Faces => "faces",
        }
    }
}

/// A model file that can be installed. Multi-file models (a network plus its
/// tokenizer) are expressed as several entries sharing a `bundle`.
#[derive(Debug, Clone, Copy)]
pub struct CatalogEntry {
    pub id: &'static str,
    pub bundle: &'static str,
    pub kind: ModelKind,
    pub version: &'static str,
    /// Filename on disk under the models directory.
    pub file_name: &'static str,
    pub url: &'static str,
    pub sha256: &'static str,
    pub size_bytes: u64,
    /// Embedding width for entries that produce vectors.
    pub dim: Option<i64>,
    pub license: &'static str,
}

/// Semantic search bundle: CLIP ViT-B/32, exported to ONNX by Qdrant.
/// 512-dimensional shared image/text space, MIT licensed.
pub const SEMANTIC_BUNDLE: &str = "clip-vit-b32";

/// On-device OCR bundle: RapidOCR's PP-OCRv4 mobile det+rec + charset.
pub const OCR_BUNDLE: &str = "rapidocr-ppv4";

/// On-device faces bundle: InsightFace buffalo_l (SCRFD-10G + ArcFace w600k_r50).
/// Non-commercial research licence — see InsightFace model zoo.
pub const FACES_BUNDLE: &str = "insightface-buffalo-l";

pub const CATALOG: &[CatalogEntry] = &[
    CatalogEntry {
        id: "clip-vit-b32-image",
        bundle: SEMANTIC_BUNDLE,
        kind: ModelKind::ClipImage,
        version: "1",
        file_name: "clip-vit-b32-image.onnx",
        url: "https://huggingface.co/Qdrant/clip-ViT-B-32-vision/resolve/main/model.onnx",
        sha256: "c68d3d9a200ddd2a8c8a5510b576d4c94d1ae383bf8b36dd8c084f94e1fb4d63",
        size_bytes: 351_686_194,
        dim: Some(512),
        license: "MIT",
    },
    CatalogEntry {
        id: "clip-vit-b32-text",
        bundle: SEMANTIC_BUNDLE,
        kind: ModelKind::ClipText,
        version: "1",
        file_name: "clip-vit-b32-text.onnx",
        url: "https://huggingface.co/Qdrant/clip-ViT-B-32-text/resolve/main/model.onnx",
        sha256: "4dbe762b11e36488304471e439cde89da053ad7acaddbf9e096745d142ec8d8b",
        size_bytes: 254_102_519,
        dim: Some(512),
        license: "MIT",
    },
    CatalogEntry {
        id: "clip-vit-b32-tokenizer",
        bundle: SEMANTIC_BUNDLE,
        kind: ModelKind::ClipText,
        version: "1",
        file_name: "clip-vit-b32-tokenizer.json",
        url: "https://huggingface.co/Qdrant/clip-ViT-B-32-text/resolve/main/tokenizer.json",
        sha256: "b68d571997a1f81bf521fb73806740ddb91e4ed6666cb6e996c066bb289cf55b",
        size_bytes: 2_224_147,
        dim: None,
        license: "MIT",
    },
    CatalogEntry {
        id: "ocr-ppv4-det",
        bundle: OCR_BUNDLE,
        kind: ModelKind::OcrDetect,
        version: "1",
        file_name: "ch_PP-OCRv4_det_infer.onnx",
        url: "https://huggingface.co/SWHL/RapidOCR/resolve/main/PP-OCRv4/ch_PP-OCRv4_det_infer.onnx",
        sha256: "d2a7720d45a54257208b1e13e36a8479894cb74155a5efe29462512d42f49da9",
        size_bytes: 4_745_517,
        dim: None,
        license: "Apache-2.0",
    },
    CatalogEntry {
        id: "ocr-ppv4-rec",
        bundle: OCR_BUNDLE,
        kind: ModelKind::OcrRecognize,
        version: "1",
        file_name: "ch_PP-OCRv4_rec_infer.onnx",
        url: "https://huggingface.co/SWHL/RapidOCR/resolve/main/PP-OCRv4/ch_PP-OCRv4_rec_infer.onnx",
        sha256: "48fc40f24f6d2a207a2b1091d3437eb3cc3eb6b676dc3ef9c37384005483683b",
        size_bytes: 10_857_958,
        dim: None,
        license: "Apache-2.0",
    },
    CatalogEntry {
        id: "ocr-ppv4-dict",
        bundle: OCR_BUNDLE,
        kind: ModelKind::OcrRecognize,
        version: "1",
        file_name: "ppocr_keys_v1.txt",
        url: "https://raw.githubusercontent.com/PaddlePaddle/PaddleOCR/release/2.7/ppocr/utils/ppocr_keys_v1.txt",
        sha256: "28b2362ad4ab2dc38769aa72feb535e3a9ddb3fd2a7585a05920e6393b1dc7f7",
        size_bytes: 26_249,
        dim: None,
        license: "Apache-2.0",
    },
    CatalogEntry {
        id: "face-scrfd-10g",
        bundle: FACES_BUNDLE,
        kind: ModelKind::FaceDetect,
        version: "1",
        file_name: "scrfd_10g_bnkps.onnx",
        url: "https://huggingface.co/immich-app/buffalo_l/resolve/main/detection/model.onnx",
        sha256: "5838f7fe053675b1c7a08b633df49e7af5495cee0493c7dcf6697200b85b5b91",
        size_bytes: 16_923_827,
        dim: None,
        license: "InsightFace (non-commercial research)",
    },
    CatalogEntry {
        id: "face-arcface-w600k-r50",
        bundle: FACES_BUNDLE,
        kind: ModelKind::FaceEmbed,
        version: "1",
        file_name: "arcface_w600k_r50.onnx",
        url: "https://huggingface.co/immich-app/buffalo_l/resolve/main/recognition/model.onnx",
        sha256: "4c06341c33c2ca1f86781dab0e829f88ad5b64be9fba56e56bc9ebdefc619e43",
        size_bytes: 174_383_860,
        dim: Some(512),
        license: "InsightFace (non-commercial research)",
    },
];

pub fn entry(id: &str) -> Option<&'static CatalogEntry> {
    CATALOG.iter().find(|e| e.id == id)
}

pub fn bundle(name: &str) -> impl Iterator<Item = &'static CatalogEntry> + use<'_> {
    CATALOG.iter().filter(move |e| e.bundle == name)
}

/// Total bytes a bundle costs on disk, for the install prompt.
pub fn bundle_size(name: &str) -> u64 {
    bundle(name).map(|e| e.size_bytes).sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_ids_are_unique() {
        let mut ids: Vec<&str> = CATALOG.iter().map(|e| e.id).collect();
        let total = ids.len();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), total, "duplicate catalog id");
    }

    #[test]
    fn every_entry_pins_a_full_length_sha256() {
        for e in CATALOG {
            assert_eq!(e.sha256.len(), 64, "{} has a malformed sha256", e.id);
            assert!(
                e.sha256.chars().all(|c| c.is_ascii_hexdigit()),
                "{} sha256 is not hex",
                e.id
            );
            assert!(e.size_bytes > 0, "{} has no size", e.id);
        }
    }

    #[test]
    fn every_entry_downloads_over_https() {
        for e in CATALOG {
            assert!(
                e.url.starts_with("https://"),
                "{} must download over https",
                e.id
            );
        }
    }

    #[test]
    fn file_names_stay_inside_the_models_directory() {
        for e in CATALOG {
            assert!(
                !e.file_name.contains('/') && !e.file_name.contains(".."),
                "{} file_name must be a bare filename",
                e.id
            );
        }
    }

    #[test]
    fn semantic_bundle_has_an_image_a_text_and_a_tokenizer() {
        let entries: Vec<_> = bundle(SEMANTIC_BUNDLE).collect();
        assert_eq!(entries.len(), 3);
        assert!(entries.iter().any(|e| e.kind == ModelKind::ClipImage));
        assert!(entries.iter().any(|e| e.file_name.ends_with(".json")));
        assert!(bundle_size(SEMANTIC_BUNDLE) > 500_000_000);
    }

    #[test]
    fn ocr_bundle_has_det_rec_and_dict() {
        let entries: Vec<_> = bundle(OCR_BUNDLE).collect();
        assert_eq!(entries.len(), 3);
        assert!(entries.iter().any(|e| e.kind == ModelKind::OcrDetect));
        assert!(entries.iter().any(|e| e.file_name.ends_with(".txt")));
        assert!(bundle_size(OCR_BUNDLE) > 10_000_000);
        assert!(bundle_size(OCR_BUNDLE) < 20_000_000);
    }

    #[test]
    fn embedding_models_agree_on_dimension() {
        let dims: Vec<i64> = bundle(SEMANTIC_BUNDLE).filter_map(|e| e.dim).collect();
        assert!(dims.iter().all(|d| *d == 512), "got {dims:?}");
    }

    #[test]
    fn faces_bundle_has_det_and_rec() {
        let entries: Vec<_> = bundle(FACES_BUNDLE).collect();
        assert_eq!(entries.len(), 2);
        assert!(entries.iter().any(|e| e.kind == ModelKind::FaceDetect));
        assert!(entries.iter().any(|e| e.kind == ModelKind::FaceEmbed));
        let dims: Vec<i64> = entries.iter().filter_map(|e| e.dim).collect();
        assert_eq!(dims, vec![512]);
        assert!(bundle_size(FACES_BUNDLE) > 180_000_000);
        assert!(bundle_size(FACES_BUNDLE) < 220_000_000);
    }
}
