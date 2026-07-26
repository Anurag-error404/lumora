//! CLIP ViT-B/32 inference via ONNX Runtime.
//!
//! Loads the Qdrant-exported vision and text graphs and produces L2-ready
//! 512-d embeddings. Callers still run [`crate::ml::vector::normalize`] before
//! storing — the model itself does not guarantee unit length.

use std::path::Path;

use ort::session::Session;
use ort::value::Tensor;
use parking_lot::Mutex;
use tokenizers::{PaddingParams, PaddingStrategy, Tokenizer, TruncationParams};

use crate::error::{AppError, AppResult};
use crate::ml::preprocess::{self, IMAGE_SIZE};
use crate::semantic::SemanticModelPaths;

/// CLIP context length for ViT-B/32.
const MAX_TOKENS: usize = 77;

/// On-demand CLIP sessions. Locked because `Session::run` needs `&mut self`.
pub struct ClipEngine {
    image: Mutex<Session>,
    text: Mutex<Session>,
    tokenizer: Tokenizer,
}

impl ClipEngine {
    pub fn load(paths: &SemanticModelPaths) -> AppResult<Self> {
        let image = load_session(&paths.image, "image")?;
        let text = load_session(&paths.text, "text")?;
        let tokenizer = load_tokenizer(&paths.tokenizer)?;
        Ok(Self {
            image: Mutex::new(image),
            text: Mutex::new(text),
            tokenizer,
        })
    }

    /// Embed a photo from disk. Returns a 512-d vector (not yet normalised).
    pub fn embed_image_path(&self, path: &Path) -> AppResult<Vec<f32>> {
        let pixels = preprocess::pixel_values_from_path(path)?;
        self.embed_image_pixels(&pixels)
    }

    pub fn embed_image_pixels(&self, pixels: &[f32]) -> AppResult<Vec<f32>> {
        let expected = (3 * IMAGE_SIZE * IMAGE_SIZE) as usize;
        if pixels.len() != expected {
            return Err(AppError::msg(format!(
                "pixel_values length {}, expected {expected}",
                pixels.len()
            )));
        }
        let input = Tensor::from_array(([1usize, 3, IMAGE_SIZE as usize, IMAGE_SIZE as usize], pixels.to_vec()))
            .map_err(|e| AppError::msg(format!("image tensor: {e}")))?;
        let mut session = self.image.lock();
        let outputs = session
            .run(ort::inputs!["pixel_values" => input])
            .map_err(|e| AppError::msg(format!("image inference failed: {e}")))?;
        extract_embedding(&outputs, "image_embeds")
    }

    /// Embed a natural-language query. Returns a 512-d vector (not yet normalised).
    pub fn embed_text(&self, text: &str) -> AppResult<Vec<f32>> {
        let (ids, mask) = tokenize(&self.tokenizer, text)?;
        let ids_t = Tensor::from_array(([1usize, MAX_TOKENS], ids))
            .map_err(|e| AppError::msg(format!("input_ids tensor: {e}")))?;
        let mask_t = Tensor::from_array(([1usize, MAX_TOKENS], mask))
            .map_err(|e| AppError::msg(format!("attention_mask tensor: {e}")))?;
        let mut session = self.text.lock();
        let outputs = session
            .run(ort::inputs![
                "input_ids" => ids_t,
                "attention_mask" => mask_t
            ])
            .map_err(|e| AppError::msg(format!("text inference failed: {e}")))?;
        extract_embedding(&outputs, "text_embeds")
    }
}

fn load_session(path: &Path, label: &str) -> AppResult<Session> {
    Session::builder()
        .map_err(|e| AppError::msg(format!("ort session builder ({label}): {e}")))?
        .commit_from_file(path)
        .map_err(|e| {
            AppError::msg(format!(
                "failed to load {label} model from {}: {e}",
                path.display()
            ))
        })
}

fn load_tokenizer(path: &Path) -> AppResult<Tokenizer> {
    let mut tok = Tokenizer::from_file(path).map_err(|e| {
        AppError::msg(format!("failed to load tokenizer from {}: {e}", path.display()))
    })?;
    tok.with_truncation(Some(TruncationParams {
        max_length: MAX_TOKENS,
        ..Default::default()
    }))
    .map_err(|e| AppError::msg(format!("tokenizer truncation: {e}")))?;
    // OpenAI CLIP / transformers pad with the EOS id (49407). The attention
    // mask tells the model which positions are real.
    tok.with_padding(Some(PaddingParams {
        strategy: PaddingStrategy::Fixed(MAX_TOKENS),
        pad_id: 49407,
        pad_token: "<|endoftext|>".into(),
        ..Default::default()
    }));
    Ok(tok)
}

fn tokenize(tok: &Tokenizer, text: &str) -> AppResult<(Vec<i64>, Vec<i64>)> {
    let encoding = tok
        .encode(text, true)
        .map_err(|e| AppError::msg(format!("tokenize failed: {e}")))?;
    let mut ids: Vec<i64> = encoding.get_ids().iter().map(|&id| id as i64).collect();
    let mut mask: Vec<i64> = encoding
        .get_attention_mask()
        .iter()
        .map(|&m| m as i64)
        .collect();
    // Defence in depth: the tokenizer should already pad to MAX_TOKENS, but a
    // misconfigured tokenizer.json must never feed the wrong width to ONNX.
    ids.resize(MAX_TOKENS, 0);
    mask.resize(MAX_TOKENS, 0);
    Ok((ids, mask))
}

fn extract_embedding(
    outputs: &ort::session::SessionOutputs<'_>,
    name: &str,
) -> AppResult<Vec<f32>> {
    let value = outputs
        .get(name)
        .ok_or_else(|| AppError::msg(format!("model did not produce '{name}'")))?;
    let (_shape, data) = value
        .try_extract_tensor::<f32>()
        .map_err(|e| AppError::msg(format!("reading '{name}': {e}")))?;
    // Batch dimension is always 1 for our callers.
    if data.len() < 512 {
        return Err(AppError::msg(format!(
            "'{name}' has {} floats, expected at least 512",
            data.len()
        )));
    }
    Ok(data[..512].to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ml::vector;
    use image::{Rgb, RgbImage};
    use std::path::PathBuf;

    fn models_dir() -> PathBuf {
        PathBuf::from(std::env::var("HOME").unwrap())
            .join("Library/Application Support/com.photovault.ai/models")
    }

    fn paths_if_present() -> Option<SemanticModelPaths> {
        let dir = models_dir();
        let image = dir.join("clip-vit-b32-image.onnx");
        let text = dir.join("clip-vit-b32-text.onnx");
        let tokenizer = dir.join("clip-vit-b32-tokenizer.json");
        if image.exists() && text.exists() && tokenizer.exists() {
            Some(SemanticModelPaths {
                image,
                text,
                tokenizer,
            })
        } else {
            None
        }
    }

    #[test]
    fn tokenize_pads_and_truncates_to_77() {
        let Some(paths) = paths_if_present() else {
            eprintln!("skipping: CLIP models not installed");
            return;
        };
        let engine = ClipEngine::load(&paths).unwrap();
        let (ids, mask) = tokenize(&engine.tokenizer, "a photo of a cat").unwrap();
        assert_eq!(ids.len(), 77);
        assert_eq!(mask.len(), 77);
        assert!(mask.contains(&1), "must mark real tokens");
        assert_eq!(ids[0], 49406, "must start with <|startoftext|>");
    }

    #[test]
    fn image_and_text_embeddings_are_512d_and_finite() {
        let Some(paths) = paths_if_present() else {
            eprintln!("skipping: CLIP models not installed");
            return;
        };
        let engine = ClipEngine::load(&paths).unwrap();

        let img = image::DynamicImage::ImageRgb8(RgbImage::from_pixel(
            320,
            240,
            Rgb([40, 120, 200]),
        ));
        let pixels = preprocess::pixel_values(&img);
        let mut image_vec = engine.embed_image_pixels(&pixels).unwrap();
        let mut text_vec = engine.embed_text("a blue rectangle").unwrap();

        assert_eq!(image_vec.len(), 512);
        assert_eq!(text_vec.len(), 512);
        assert!(image_vec.iter().all(|x| x.is_finite()));
        assert!(text_vec.iter().all(|x| x.is_finite()));

        vector::normalize(&mut image_vec);
        vector::normalize(&mut text_vec);
        let score = vector::similarity(&image_vec, &text_vec);
        // A solid blue field vs "blue rectangle" should not be nonsense, but we
        // only assert the path works — score bounds catch NaN explosions.
        assert!(
            (-1.0..=1.0).contains(&score),
            "cosine score out of range: {score}"
        );
    }

    #[test]
    fn identical_text_embeds_match_themselves() {
        let Some(paths) = paths_if_present() else {
            eprintln!("skipping: CLIP models not installed");
            return;
        };
        let engine = ClipEngine::load(&paths).unwrap();
        let mut a = engine.embed_text("sunset over the ocean").unwrap();
        let mut b = engine.embed_text("sunset over the ocean").unwrap();
        vector::normalize(&mut a);
        vector::normalize(&mut b);
        let score = vector::similarity(&a, &b);
        assert!(
            (score - 1.0).abs() < 1e-4,
            "identical text must score ~1, got {score}"
        );
    }
}
