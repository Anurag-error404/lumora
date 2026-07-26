//! Pluggable model library — which ONNX (or native) backends can power each
//! AI capability, and which one is active.
//!
//! Downloadable options share a `bundle` id with [`super::catalog`]. Native
//! options (duplicates, blur) never download.

use serde::{Deserialize, Serialize};

use super::catalog::{
    FACES_BUNDLE, FACES_BUNDLE_S, OCR_BUNDLE, OCR_BUNDLE_V3, SEMANTIC_BUNDLE, TAGS_BUNDLE,
    TAGS_BUNDLE_MEDIUM,
};

/// User-facing AI capability that can pick a backend.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Capability {
    SemanticSearch,
    Ocr,
    Faces,
    AutoTags,
    Duplicates,
    BlurDetection,
}

impl Capability {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::SemanticSearch => "semanticSearch",
            Self::Ocr => "ocr",
            Self::Faces => "faces",
            Self::AutoTags => "autoTags",
            Self::Duplicates => "duplicates",
            Self::BlurDetection => "blurDetection",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "semanticSearch" | "semantic" | "clip" | "embeddings" => Some(Self::SemanticSearch),
            "ocr" | "text" => Some(Self::Ocr),
            "faces" | "people" => Some(Self::Faces),
            "autoTags" | "tags" | "objectDetection" | "auto_tags" | "object_detection" => {
                Some(Self::AutoTags)
            }
            "duplicates" => Some(Self::Duplicates),
            "blurDetection" | "blur" => Some(Self::BlurDetection),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::SemanticSearch => "Semantic search",
            Self::Ocr => "Text recognition (OCR)",
            Self::Faces => "Face detection & recognition",
            Self::AutoTags => "Image classification / auto-tags",
            Self::Duplicates => "Duplicate detection",
            Self::BlurDetection => "Blur detection",
        }
    }
}

/// How a backend runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RuntimeKind {
    Onnx,
    Native,
}

/// One selectable backend for a capability.
#[derive(Debug, Clone, Copy)]
pub struct ModelOption {
    pub id: &'static str,
    pub capability: Capability,
    /// Catalog bundle to install, when `runtime == Onnx`.
    pub bundle: Option<&'static str>,
    pub name: &'static str,
    pub summary: &'static str,
    pub runtime: RuntimeKind,
    pub license: &'static str,
    /// Classifier input edge length (auto-tags only).
    pub input_size: Option<u32>,
    pub default: bool,
}

pub const LIBRARY: &[ModelOption] = &[
    ModelOption {
        id: "clip-vit-b32",
        capability: Capability::SemanticSearch,
        bundle: Some(SEMANTIC_BUNDLE),
        name: "CLIP ViT-B/32",
        summary: "Balanced quality and speed. 512-d shared image/text space.",
        runtime: RuntimeKind::Onnx,
        license: "MIT",
        input_size: None,
        default: true,
    },
    ModelOption {
        id: "rapidocr-ppv4",
        capability: Capability::Ocr,
        bundle: Some(OCR_BUNDLE),
        name: "RapidOCR PP-OCRv4",
        summary: "Current default. Strong general OCR on screenshots and documents.",
        runtime: RuntimeKind::Onnx,
        license: "Apache-2.0",
        input_size: None,
        default: true,
    },
    ModelOption {
        id: "rapidocr-ppv3",
        capability: Capability::Ocr,
        bundle: Some(OCR_BUNDLE_V3),
        name: "RapidOCR PP-OCRv3",
        summary: "Smaller / faster mobile OCR. Slightly lower accuracy than v4.",
        runtime: RuntimeKind::Onnx,
        license: "Apache-2.0",
        input_size: None,
        default: false,
    },
    ModelOption {
        id: "insightface-buffalo-l",
        capability: Capability::Faces,
        bundle: Some(FACES_BUNDLE),
        name: "InsightFace buffalo_l",
        summary: "SCRFD-10G + ArcFace r50. Highest quality; larger download.",
        runtime: RuntimeKind::Onnx,
        license: "InsightFace (non-commercial research)",
        input_size: None,
        default: true,
    },
    ModelOption {
        id: "insightface-buffalo-s",
        capability: Capability::Faces,
        bundle: Some(FACES_BUNDLE_S),
        name: "InsightFace buffalo_s",
        summary: "Lighter SCRFD + ArcFace. Faster on CPU; slightly less accurate.",
        runtime: RuntimeKind::Onnx,
        license: "InsightFace (non-commercial research)",
        input_size: None,
        default: false,
    },
    ModelOption {
        id: "mobilenetv4-small",
        capability: Capability::AutoTags,
        bundle: Some(TAGS_BUNDLE),
        name: "MobileNetV4 Small",
        summary: "Fast ImageNet classifier at 224². Good default for auto-tags.",
        runtime: RuntimeKind::Onnx,
        license: "Apache-2.0",
        input_size: Some(224),
        default: true,
    },
    ModelOption {
        id: "mobilenetv4-medium",
        capability: Capability::AutoTags,
        bundle: Some(TAGS_BUNDLE_MEDIUM),
        name: "MobileNetV4 Medium",
        summary: "Higher capacity at 256². Slower, often more accurate labels.",
        runtime: RuntimeKind::Onnx,
        license: "Apache-2.0",
        input_size: Some(256),
        default: false,
    },
    ModelOption {
        id: "sha256-phash",
        capability: Capability::Duplicates,
        bundle: None,
        name: "SHA256 + perceptual hash",
        summary: "Exact and near-duplicate grouping. Always on; no download.",
        runtime: RuntimeKind::Native,
        license: "Built-in",
        input_size: None,
        default: true,
    },
    ModelOption {
        id: "laplacian-variance",
        capability: Capability::BlurDetection,
        bundle: None,
        name: "Variance of Laplacian",
        summary: "Classic blur score from local image statistics. Native; no download.",
        runtime: RuntimeKind::Native,
        license: "Built-in",
        input_size: None,
        default: true,
    },
];

pub fn option(id: &str) -> Option<&'static ModelOption> {
    LIBRARY.iter().find(|o| o.id == id)
}

pub fn options_for(capability: Capability) -> impl Iterator<Item = &'static ModelOption> {
    LIBRARY.iter().filter(move |o| o.capability == capability)
}

pub fn default_option(capability: Capability) -> &'static ModelOption {
    options_for(capability)
        .find(|o| o.default)
        .or_else(|| options_for(capability).next())
        .expect("every capability has at least one library option")
}

pub fn resolve_active(capability: Capability, preferred: &str) -> &'static ModelOption {
    options_for(capability)
        .find(|o| o.id == preferred)
        .unwrap_or_else(|| default_option(capability))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_capability_has_a_default() {
        for cap in [
            Capability::SemanticSearch,
            Capability::Ocr,
            Capability::Faces,
            Capability::AutoTags,
            Capability::Duplicates,
            Capability::BlurDetection,
        ] {
            let d = default_option(cap);
            assert_eq!(d.capability, cap);
            assert!(d.default || options_for(cap).count() == 1);
        }
    }

    #[test]
    fn onnx_options_point_at_known_bundles() {
        for o in LIBRARY {
            if let Some(bundle) = o.bundle {
                assert!(
                    crate::ml::catalog::bundle(bundle).next().is_some(),
                    "{} references unknown bundle {bundle}",
                    o.id
                );
            }
        }
    }

    #[test]
    fn faces_and_ocr_and_tags_have_alternates() {
        assert!(options_for(Capability::Faces).count() >= 2);
        assert!(options_for(Capability::Ocr).count() >= 2);
        assert!(options_for(Capability::AutoTags).count() >= 2);
    }
}
