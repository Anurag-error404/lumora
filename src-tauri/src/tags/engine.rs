//! MobileNetV4 ImageNet classifier via ONNX Runtime.

use std::path::Path;

use image::imageops::{self, FilterType};
use image::DynamicImage;
use ort::session::Session;
use ort::value::Tensor;
use parking_lot::Mutex;

use crate::error::{AppError, AppResult};
use crate::tags::{display_label, TagsModelPaths};
use crate::thumbnails;

/// ImageNet mean/std from the MobileNetV4 preprocessor config.
const MEAN: [f32; 3] = [0.485, 0.456, 0.406];
const STD: [f32; 3] = [0.229, 0.224, 0.225];
const TOP_K: usize = 5;
const MIN_SCORE: f32 = 0.05;

pub struct TagsEngine {
    session: Mutex<Session>,
    input_name: String,
    output_name: String,
    labels: Vec<String>,
    input_size: u32,
    resize_short: u32,
}

impl TagsEngine {
    pub fn load(paths: &TagsModelPaths) -> AppResult<Self> {
        let session = Session::builder()
            .map_err(|e| AppError::msg(format!("ort session builder: {e}")))?
            .commit_from_file(&paths.model)
            .map_err(|e| AppError::msg(format!("failed to load MobileNetV4: {e}")))?;
        let input_name = session
            .inputs()
            .first()
            .map(|i| i.name().to_string())
            .ok_or_else(|| AppError::msg("MobileNetV4 has no inputs"))?;
        let output_name = session
            .outputs()
            .first()
            .map(|o| o.name().to_string())
            .ok_or_else(|| AppError::msg("MobileNetV4 has no outputs"))?;
        let labels = load_labels(&paths.labels)?;
        if labels.len() != 1000 {
            return Err(AppError::msg(format!(
                "expected 1000 ImageNet labels, got {}",
                labels.len()
            )));
        }
        let input_size = paths.input_size.max(64);
        // Match HF preprocessor: shortest edge slightly above crop size.
        let resize_short = if input_size >= 256 { 269 } else { 256 };
        Ok(Self {
            session: Mutex::new(session),
            input_name,
            output_name,
            labels,
            input_size,
            resize_short,
        })
    }

    pub fn run_path(&self, path: &Path) -> AppResult<Vec<(String, f32)>> {
        let img = thumbnails::open_oriented(path)?;
        self.run_image(&img)
    }

    pub fn run_image(&self, img: &DynamicImage) -> AppResult<Vec<(String, f32)>> {
        let pixels = preprocess(img, self.input_size, self.resize_short);
        let size = self.input_size as usize;
        let input = Tensor::from_array(([1usize, 3, size, size], pixels))
            .map_err(|e| AppError::msg(format!("tags tensor: {e}")))?;

        let mut session = self.session.lock();
        let outputs = session
            .run(ort::inputs![self.input_name.as_str() => input])
            .map_err(|e| AppError::msg(format!("tags inference: {e}")))?;

        let value = outputs
            .get(self.output_name.as_str())
            .ok_or_else(|| AppError::msg("MobileNetV4 missing logits output"))?;
        let (_shape, data) = value
            .try_extract_tensor::<f32>()
            .map_err(|e| AppError::msg(format!("tags output: {e}")))?;
        let logits: Vec<f32> = data.iter().copied().collect();
        if logits.len() < self.labels.len() {
            return Err(AppError::msg(format!(
                "tags output len {} < labels {}",
                logits.len(),
                self.labels.len()
            )));
        }
        Ok(top_k(
            &logits[..self.labels.len()],
            &self.labels,
            TOP_K,
            MIN_SCORE,
        ))
    }
}

fn load_labels(path: &Path) -> AppResult<Vec<String>> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| AppError::msg(format!("read labels: {e}")))?;
    Ok(text
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect())
}

/// Resize shortest edge, center-crop, ImageNet normalize → NCHW.
fn preprocess(img: &DynamicImage, input_size: u32, resize_short: u32) -> Vec<f32> {
    let rgb = img.to_rgb8();
    let (w, h) = rgb.dimensions();
    let (nw, nh) = if w <= h {
        let scale = resize_short as f32 / w as f32;
        (resize_short, (h as f32 * scale).round().max(1.0) as u32)
    } else {
        let scale = resize_short as f32 / h as f32;
        ((w as f32 * scale).round().max(1.0) as u32, resize_short)
    };
    let resized = imageops::resize(&rgb, nw, nh, FilterType::CatmullRom);
    let left = nw.saturating_sub(input_size) / 2;
    let top = nh.saturating_sub(input_size) / 2;
    let cropped = imageops::crop_imm(&resized, left, top, input_size, input_size).to_image();

    let mut out = vec![0.0f32; (3 * input_size * input_size) as usize];
    let plane = (input_size * input_size) as usize;
    for y in 0..input_size {
        for x in 0..input_size {
            let p = cropped.get_pixel(x, y).0;
            let idx = (y * input_size + x) as usize;
            for c in 0..3 {
                let v = p[c] as f32 / 255.0;
                out[c * plane + idx] = (v - MEAN[c]) / STD[c];
            }
        }
    }
    out
}

fn top_k(
    logits: &[f32],
    labels: &[String],
    k: usize,
    min_score: f32,
) -> Vec<(String, f32)> {
    let probs = softmax(logits);
    let mut indexed: Vec<(usize, f32)> = probs.iter().copied().enumerate().collect();
    indexed.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    let mut out: Vec<(String, f32)> = indexed
        .iter()
        .take(k)
        .filter(|(_, s)| *s >= min_score)
        .map(|(i, s)| (display_label(&labels[*i]), *s))
        .collect();
    // Always keep the top prediction so a low-confidence photo is still marked
    // processed and searchable, instead of stalling the queue as "pending".
    if out.is_empty() {
        if let Some((i, s)) = indexed.first() {
            out.push((display_label(&labels[*i]), *s));
        }
    }
    out
}

fn softmax(logits: &[f32]) -> Vec<f32> {
    let max = logits.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    let exps: Vec<f32> = logits.iter().map(|x| (x - max).exp()).collect();
    let sum: f32 = exps.iter().sum();
    if sum <= 0.0 || !sum.is_finite() {
        return vec![0.0; logits.len()];
    }
    exps.into_iter().map(|e| e / sum).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn softmax_sums_to_one() {
        let p = softmax(&[1.0, 2.0, 3.0]);
        let sum: f32 = p.iter().sum();
        assert!((sum - 1.0).abs() < 1e-5);
    }

    #[test]
    fn top_k_picks_highest() {
        let labels: Vec<String> = (0..5).map(|i| format!("c{i}")).collect();
        let logits = [0.1f32, 5.0, 0.2, 1.0, 0.0];
        let top = top_k(&logits, &labels, 2, 0.01);
        assert_eq!(top[0].0, "c1");
    }

    #[test]
    fn top_k_keeps_best_even_below_min_score() {
        let labels: Vec<String> = (0..5).map(|i| format!("c{i}")).collect();
        // Flat logits → ~0.2 each after softmax, all below 0.5.
        let logits = [1.0f32, 1.0, 1.0, 1.0, 1.0];
        let top = top_k(&logits, &labels, 5, 0.5);
        assert_eq!(top.len(), 1, "must keep a top-1 so jobs leave the queue");
    }
}
