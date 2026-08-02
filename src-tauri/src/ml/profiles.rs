//! Compatibility profiles for bring-your-own ONNX models.

use super::library::Capability;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProfileKind {
    AutoTags,
    ClipVision,
    ClipText,
}

impl ProfileKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::AutoTags => "autoTags",
            Self::ClipVision => "clipVision",
            Self::ClipText => "clipText",
        }
    }

    #[allow(dead_code)]
    pub fn supports_capability(cap: Capability) -> bool {
        matches!(
            cap,
            Capability::AutoTags | Capability::SemanticSearch
        )
    }
}

/// What we accept for ImageNet-style auto-tag classifiers.
pub const AUTOTAGS_LABEL_COUNT: usize = 1000;
pub const AUTOTAGS_INPUT_SIZES: &[u32] = &[224, 256];

/// CLIP ViT-B/32 export contract (Qdrant / OpenAI-compat ONNX).
pub const CLIP_IMAGE_SIZE: u32 = 224;
pub const CLIP_SEQ_LEN: usize = 77;
pub const CLIP_EMBED_DIM: usize = 512;
pub const CLIP_PIXEL_VALUES: &str = "pixel_values";
pub const CLIP_IMAGE_EMBEDS: &str = "image_embeds";
pub const CLIP_INPUT_IDS: &str = "input_ids";
pub const CLIP_ATTENTION_MASK: &str = "attention_mask";
pub const CLIP_TEXT_EMBEDS: &str = "text_embeds";
