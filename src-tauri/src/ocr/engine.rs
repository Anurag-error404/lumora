//! PP-OCRv4 ONNX inference: detection + recognition + CTC decode.

use std::path::Path;

use image::imageops::{self, FilterType};
use image::{DynamicImage, RgbaImage};
use ort::session::Session;
use ort::value::Tensor;
use parking_lot::Mutex;

use crate::error::{AppError, AppResult};
use crate::ocr::OcrModelPaths;
use crate::thumbnails;

const DET_LIMIT: u32 = 960;
const DET_THRESH: f32 = 0.3;
const BOX_EXPAND: f32 = 1.5;
const MIN_BOX: u32 = 8;
const REC_HEIGHT: u32 = 48;
const REC_MAX_WIDTH: u32 = 320;
const MAX_BOXES: usize = 64;

const DET_MEAN: [f32; 3] = [0.485, 0.456, 0.406];
const DET_STD: [f32; 3] = [0.229, 0.224, 0.225];

#[derive(Debug, Clone, Copy)]
pub struct TextBox {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

#[derive(Debug, Clone)]
pub struct OcrResult {
    pub text: String,
    pub confidence: f32,
}

pub struct OcrEngine {
    det: Mutex<Session>,
    rec: Mutex<Session>,
    det_input: String,
    det_output: String,
    rec_input: String,
    rec_output: String,
    charset: Vec<String>,
}

impl OcrEngine {
    pub fn load(paths: &OcrModelPaths) -> AppResult<Self> {
        let det = load_session(&paths.det, "ocr-det")?;
        let rec = load_session(&paths.rec, "ocr-rec")?;
        let (det_input, det_output) = io_names(&det)?;
        let (rec_input, rec_output) = io_names(&rec)?;
        let charset = load_charset(&paths.dict)?;
        Ok(Self {
            det: Mutex::new(det),
            rec: Mutex::new(rec),
            det_input,
            det_output,
            rec_input,
            rec_output,
            charset,
        })
    }

    /// Run full OCR on an image path (EXIF-oriented).
    pub fn run_path(&self, path: &Path) -> AppResult<OcrResult> {
        let img = thumbnails::open_oriented(path)?;
        self.run_image(&img)
    }

    pub fn run_image(&self, img: &DynamicImage) -> AppResult<OcrResult> {
        let rgba = img.to_rgba8();
        let (orig_w, orig_h) = rgba.dimensions();
        if orig_w < 8 || orig_h < 8 {
            return Ok(OcrResult {
                text: String::new(),
                confidence: 0.0,
            });
        }

        let boxes = self.detect(&rgba)?;
        if boxes.is_empty() {
            return Ok(OcrResult {
                text: String::new(),
                confidence: 0.0,
            });
        }

        let mut lines: Vec<(String, f32, u32)> = Vec::new();
        for b in boxes.iter().take(MAX_BOXES) {
            let crop = crop_box(&rgba, *b);
            if crop.width() < 4 || crop.height() < 4 {
                continue;
            }
            match self.recognize(&crop) {
                Ok((text, conf)) if !text.trim().is_empty() => {
                    lines.push((text, conf, b.y));
                }
                _ => continue,
            }
        }
        lines.sort_by_key(|(_, _, y)| *y);

        let confs: Vec<f32> = lines.iter().map(|(_, c, _)| *c).collect();
        let confidence = if confs.is_empty() {
            0.0
        } else {
            confs.iter().sum::<f32>() / confs.len() as f32
        };
        let text = lines
            .into_iter()
            .map(|(t, _, _)| t)
            .collect::<Vec<_>>()
            .join("\n");
        Ok(OcrResult { text, confidence })
    }

    fn detect(&self, rgba: &RgbaImage) -> AppResult<Vec<TextBox>> {
        let (orig_w, orig_h) = rgba.dimensions();
        let (nw, nh, scale) = det_resize_dims(orig_w, orig_h);
        let resized = imageops::resize(rgba, nw, nh, FilterType::Triangle);
        let pixels = det_pixel_values(&resized);
        let input = Tensor::from_array(([1usize, 3, nh as usize, nw as usize], pixels))
            .map_err(|e| AppError::msg(format!("ocr det tensor: {e}")))?;

        let mut session = self.det.lock();
        let outputs = session
            .run(ort::inputs![self.det_input.as_str() => input])
            .map_err(|e| AppError::msg(format!("ocr det inference: {e}")))?;
        let value = outputs.get(self.det_output.as_str()).ok_or_else(|| {
            AppError::msg(format!("ocr det missing output '{}'", self.det_output))
        })?;
        let (shape, data) = value
            .try_extract_tensor::<f32>()
            .map_err(|e| AppError::msg(format!("ocr det output: {e}")))?;

        // Expected [1, 1, H, W] or [1, H, W].
        let (mh, mw) = match shape.len() {
            4 => (shape[2] as u32, shape[3] as u32),
            3 => (shape[1] as u32, shape[2] as u32),
            2 => (shape[0] as u32, shape[1] as u32),
            _ => {
                return Err(AppError::msg(format!(
                    "unexpected det output rank {}",
                    shape.len()
                )))
            }
        };
        if (mh * mw) as usize > data.len() {
            return Err(AppError::msg("det output shorter than HxW"));
        }

        let binary: Vec<u8> = data[..(mh * mw) as usize]
            .iter()
            .map(|&v| if v >= DET_THRESH { 1 } else { 0 })
            .collect();
        let comps = connected_components(&binary, mw, mh);
        let mut boxes = Vec::new();
        for (x0, y0, x1, y1) in comps {
            let bw = x1.saturating_sub(x0).max(1);
            let bh = y1.saturating_sub(y0).max(1);
            // Expand around center (unclip-lite).
            let cx = x0 as f32 + bw as f32 / 2.0;
            let cy = y0 as f32 + bh as f32 / 2.0;
            let ew = (bw as f32 * BOX_EXPAND).max(MIN_BOX as f32);
            let eh = (bh as f32 * BOX_EXPAND).max(MIN_BOX as f32);
            let mut bx0 = ((cx - ew / 2.0) / scale).round() as i64;
            let mut by0 = ((cy - eh / 2.0) / scale).round() as i64;
            let mut bx1 = ((cx + ew / 2.0) / scale).round() as i64;
            let mut by1 = ((cy + eh / 2.0) / scale).round() as i64;
            bx0 = bx0.clamp(0, orig_w as i64 - 1);
            by0 = by0.clamp(0, orig_h as i64 - 1);
            bx1 = bx1.clamp(bx0 + 1, orig_w as i64);
            by1 = by1.clamp(by0 + 1, orig_h as i64);
            let w = (bx1 - bx0) as u32;
            let h = (by1 - by0) as u32;
            if w >= MIN_BOX && h >= 4 {
                boxes.push(TextBox {
                    x: bx0 as u32,
                    y: by0 as u32,
                    w,
                    h,
                });
            }
        }
        // Prefer larger / higher boxes first for stability.
        boxes.sort_by(|a, b| a.y.cmp(&b.y).then_with(|| b.w.cmp(&a.w)));
        Ok(boxes)
    }

    fn recognize(&self, crop: &RgbaImage) -> AppResult<(String, f32)> {
        let (pixels, width) = rec_pixel_values(crop);
        let input = Tensor::from_array(([1usize, 3, REC_HEIGHT as usize, width], pixels))
            .map_err(|e| AppError::msg(format!("ocr rec tensor: {e}")))?;
        let mut session = self.rec.lock();
        let outputs = session
            .run(ort::inputs![self.rec_input.as_str() => input])
            .map_err(|e| AppError::msg(format!("ocr rec inference: {e}")))?;
        let value = outputs.get(self.rec_output.as_str()).ok_or_else(|| {
            AppError::msg(format!("ocr rec missing output '{}'", self.rec_output))
        })?;
        let (shape, data) = value
            .try_extract_tensor::<f32>()
            .map_err(|e| AppError::msg(format!("ocr rec output: {e}")))?;
        ctc_greedy_decode(data, &shape[..], &self.charset)
    }
}

fn load_session(path: &Path, label: &str) -> AppResult<Session> {
    crate::ml::session::load_session(path, label)
}

fn io_names(session: &Session) -> AppResult<(String, String)> {
    let input = session
        .inputs()
        .first()
        .map(|i| i.name().to_string())
        .ok_or_else(|| AppError::msg("ocr model has no inputs"))?;
    let output = session
        .outputs()
        .first()
        .map(|o| o.name().to_string())
        .ok_or_else(|| AppError::msg("ocr model has no outputs"))?;
    Ok((input, output))
}

pub fn load_charset(path: &Path) -> AppResult<Vec<String>> {
    let raw = std::fs::read_to_string(path)
        .map_err(|e| AppError::msg(format!("read OCR dict {}: {e}", path.display())))?;
    // Index 0 is the CTC blank; characters start at 1.
    let mut chars = vec![String::new()];
    for line in raw.lines() {
        // Preserve empty lines as blank? PP-OCR dict has one char per line.
        chars.push(line.to_string());
    }
    if chars.len() < 2 {
        return Err(AppError::msg("OCR dictionary looks empty"));
    }
    Ok(chars)
}

fn det_resize_dims(w: u32, h: u32) -> (u32, u32, f32) {
    let max_side = w.max(h) as f32;
    let scale = if max_side > DET_LIMIT as f32 {
        DET_LIMIT as f32 / max_side
    } else {
        1.0
    };
    let mut nw = ((w as f32 * scale) as u32).max(32);
    let mut nh = ((h as f32 * scale) as u32).max(32);
    nw = (nw + 31) / 32 * 32;
    nh = (nh + 31) / 32 * 32;
    let scale_x = nw as f32 / w as f32;
    (nw, nh, scale_x)
}

fn det_pixel_values(img: &RgbaImage) -> Vec<f32> {
    let (w, h) = img.dimensions();
    let mut out = vec![0.0f32; (3 * w * h) as usize];
    let plane = (w * h) as usize;
    for y in 0..h {
        for x in 0..w {
            let p = img.get_pixel(x, y).0;
            let idx = (y * w + x) as usize;
            for c in 0..3 {
                let v = p[c] as f32 / 255.0;
                out[c * plane + idx] = (v - DET_MEAN[c]) / DET_STD[c];
            }
        }
    }
    out
}

fn rec_pixel_values(img: &RgbaImage) -> (Vec<f32>, usize) {
    let (w, h) = img.dimensions();
    let scale = REC_HEIGHT as f32 / h.max(1) as f32;
    let mut nw = ((w as f32 * scale).round() as u32).max(8);
    nw = nw.min(REC_MAX_WIDTH);
    // Width multiple of 8 keeps some models happier.
    nw = (nw + 7) / 8 * 8;
    let resized = imageops::resize(img, nw, REC_HEIGHT, FilterType::Triangle);
    let plane = (nw * REC_HEIGHT) as usize;
    let mut out = vec![0.0f32; 3 * plane];
    for y in 0..REC_HEIGHT {
        for x in 0..nw {
            let p = resized.get_pixel(x, y).0;
            let idx = (y * nw + x) as usize;
            for c in 0..3 {
                let v = p[c] as f32 / 255.0;
                out[c * plane + idx] = (v - 0.5) / 0.5;
            }
        }
    }
    (out, nw as usize)
}

fn crop_box(img: &RgbaImage, b: TextBox) -> RgbaImage {
    let (iw, ih) = img.dimensions();
    let x = b.x.min(iw.saturating_sub(1));
    let y = b.y.min(ih.saturating_sub(1));
    let w = b.w.min(iw.saturating_sub(x)).max(1);
    let h = b.h.min(ih.saturating_sub(y)).max(1);
    imageops::crop_imm(img, x, y, w, h).to_image()
}

/// 4-connected component bounding boxes on a binary map.
pub fn connected_components(map: &[u8], width: u32, height: u32) -> Vec<(u32, u32, u32, u32)> {
    let w = width as usize;
    let h = height as usize;
    let mut visited = vec![false; w * h];
    let mut boxes = Vec::new();
    let mut stack = Vec::new();

    for y in 0..h {
        for x in 0..w {
            let i = y * w + x;
            if map[i] == 0 || visited[i] {
                continue;
            }
            stack.clear();
            stack.push((x, y));
            visited[i] = true;
            let mut min_x = x;
            let mut max_x = x;
            let mut min_y = y;
            let mut max_y = y;
            let mut area = 0usize;
            while let Some((cx, cy)) = stack.pop() {
                area += 1;
                min_x = min_x.min(cx);
                max_x = max_x.max(cx);
                min_y = min_y.min(cy);
                max_y = max_y.max(cy);
                for (nx, ny) in [
                    (cx.wrapping_sub(1), cy),
                    (cx + 1, cy),
                    (cx, cy.wrapping_sub(1)),
                    (cx, cy + 1),
                ] {
                    if nx >= w || ny >= h {
                        continue;
                    }
                    let ni = ny * w + nx;
                    if map[ni] != 0 && !visited[ni] {
                        visited[ni] = true;
                        stack.push((nx, ny));
                    }
                }
            }
            // Drop tiny speckles (noise in the probability map).
            if area >= 6 {
                boxes.push((
                    min_x as u32,
                    min_y as u32,
                    (max_x + 1) as u32,
                    (max_y + 1) as u32,
                ));
            }
        }
    }
    boxes
}

/// CTC greedy decode. `data` is logits or probabilities shaped [T, C] or [1, T, C] or [1, C, T].
pub fn ctc_greedy_decode(
    data: &[f32],
    shape: &[i64],
    charset: &[String],
) -> AppResult<(String, f32)> {
    let (time, classes, layout) = match shape.len() {
        2 => (shape[0] as usize, shape[1] as usize, "tc"),
        3 if shape[0] == 1 => {
            // [1, T, C] vs [1, C, T] — prefer larger dim as T when C ≈ charset size.
            let a = shape[1] as usize;
            let b = shape[2] as usize;
            if (a as i64 - charset.len() as i64).abs() < (b as i64 - charset.len() as i64).abs() {
                (b, a, "ct") // [1, C, T]
            } else {
                (a, b, "tc") // [1, T, C]
            }
        }
        _ => return Err(AppError::msg(format!("unexpected rec shape {shape:?}"))),
    };
    if classes == 0 || time == 0 || data.len() < time * classes {
        return Err(AppError::msg("rec output too small for CTC"));
    }

    let mut text = String::new();
    let mut conf_sum = 0.0f32;
    let mut conf_n = 0u32;
    let mut prev = usize::MAX;
    for t in 0..time {
        let mut best_i = 0usize;
        let mut best_v = f32::NEG_INFINITY;
        for c in 0..classes {
            let v = match layout {
                "tc" => data[t * classes + c],
                _ => data[c * time + t],
            };
            if v > best_v {
                best_v = v;
                best_i = c;
            }
        }
        if best_i != 0 && best_i != prev {
            if let Some(ch) = charset.get(best_i) {
                text.push_str(ch);
                // Softmax-ish confidence: if logits, approx via exp; if already
                // probs, best_v is fine. Clamp to [0,1].
                let conf = if best_v > 1.0 || best_v < 0.0 {
                    // crude: compare against next-best would be better; use sigmoid-ish
                    1.0 / (1.0 + (-best_v).exp())
                } else {
                    best_v
                };
                conf_sum += conf.clamp(0.0, 1.0);
                conf_n += 1;
            }
        }
        prev = best_i;
    }
    let confidence = if conf_n == 0 {
        0.0
    } else {
        conf_sum / conf_n as f32
    };
    Ok((text, confidence))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn charset_index_zero_is_blank() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("dict.txt");
        std::fs::write(&path, "a\nb\nc\n").unwrap();
        let cs = load_charset(&path).unwrap();
        assert_eq!(cs[0], "");
        assert_eq!(cs[1], "a");
        assert_eq!(cs[3], "c");
    }

    #[test]
    fn ctc_skips_blanks_and_duplicates() {
        // charset: blank, A, B  → classes=3
        let charset = vec!["".into(), "A".into(), "B".into()];
        // T=5, C=3, sequence: blank, A, A, blank, B  → "AB"
        // logits one-hot-ish
        let mut data = vec![0.0f32; 5 * 3];
        // t0 blank
        data[0 * 3 + 0] = 5.0;
        // t1 A
        data[1 * 3 + 1] = 5.0;
        // t2 A (dup)
        data[2 * 3 + 1] = 5.0;
        // t3 blank
        data[3 * 3 + 0] = 5.0;
        // t4 B
        data[4 * 3 + 2] = 5.0;
        let (text, conf) = ctc_greedy_decode(&data, &[5, 3], &charset).unwrap();
        assert_eq!(text, "AB");
        assert!(conf > 0.5);
    }

    #[test]
    fn connected_components_finds_two_blobs() {
        // 8x4 map with two rectangles
        let w = 8u32;
        let h = 4u32;
        let mut map = vec![0u8; (w * h) as usize];
        for y in 0..2 {
            for x in 0..3 {
                map[(y * w + x) as usize] = 1;
            }
        }
        for y in 2..4 {
            for x in 5..8 {
                map[(y * w + x) as usize] = 1;
            }
        }
        let boxes = connected_components(&map, w, h);
        assert_eq!(boxes.len(), 2);
    }

    #[test]
    fn engine_loads_when_models_present() {
        let dir = PathBuf::from(std::env::var("HOME").unwrap())
            .join("Library/Application Support/com.photovault.ai/models");
        let paths = OcrModelPaths {
            det: dir.join("ch_PP-OCRv4_det_infer.onnx"),
            rec: dir.join("ch_PP-OCRv4_rec_infer.onnx"),
            dict: dir.join("ppocr_keys_v1.txt"),
        };
        if !(paths.det.exists() && paths.rec.exists() && paths.dict.exists()) {
            // Also accept the probe downloads.
            let paths = OcrModelPaths {
                det: PathBuf::from("/tmp/lumora-ocr-models/ch_PP-OCRv4_det_infer.onnx"),
                rec: PathBuf::from("/tmp/lumora-ocr-models/ch_PP-OCRv4_rec_infer.onnx"),
                dict: PathBuf::from("/tmp/lumora-ocr-models/ppocr_keys_v1.txt"),
            };
            if !(paths.det.exists() && paths.rec.exists() && paths.dict.exists()) {
                eprintln!("skipping: OCR models not installed");
                return;
            }
            let engine = OcrEngine::load(&paths).unwrap();
            let img = DynamicImage::ImageRgb8(image::RgbImage::from_pixel(
                200,
                80,
                image::Rgb([255, 255, 255]),
            ));
            let _ = engine.run_image(&img).unwrap();
            return;
        }
        let engine = OcrEngine::load(&paths).unwrap();
        let img = DynamicImage::ImageRgb8(image::RgbImage::from_pixel(
            200,
            80,
            image::Rgb([255, 255, 255]),
        ));
        let _ = engine.run_image(&img).unwrap();
    }
}
