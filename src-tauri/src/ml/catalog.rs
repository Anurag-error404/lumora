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
}

impl ModelKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ClipImage => "clip_image",
            Self::ClipText => "clip_text",
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
    fn embedding_models_agree_on_dimension() {
        let dims: Vec<i64> = bundle(SEMANTIC_BUNDLE).filter_map(|e| e.dim).collect();
        assert!(dims.iter().all(|d| *d == 512), "got {dims:?}");
    }
}
