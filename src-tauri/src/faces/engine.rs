//! SCRFD detection + ArcFace recognition via ONNX Runtime.
//!
//! Detection letterboxes to 640×640, decodes the 9 SCRFD outputs by trailing
//! dimension (score / bbox / keypoints), applies distance2bbox + NMS, then
//! similarity-aligns each face to the ArcFace 112×112 template before embedding.

use std::path::Path;

use image::imageops::{self, FilterType};
use image::{DynamicImage, ImageBuffer, Rgba, RgbaImage};
use ort::session::Session;
use ort::value::Tensor;
use parking_lot::Mutex;

use crate::error::{AppError, AppResult};
use crate::faces::FaceModelPaths;
use crate::ml::vector;
use crate::thumbnails;

const DET_SIZE: u32 = 640;
const REC_SIZE: u32 = 112;
const DET_THRESH: f32 = 0.5;
const NMS_THRESH: f32 = 0.4;
const MAX_FACES: usize = 32;
const STRIDES: [u32; 3] = [8, 16, 32];
const NUM_ANCHORS: usize = 2;

/// ArcFace 112×112 template landmarks.
const ARC_FACE_REF: [[f32; 2]; 5] = [
    [38.2946, 51.6963],
    [73.5318, 51.5014],
    [56.0252, 71.7366],
    [41.5493, 92.3655],
    [70.7299, 92.2041],
];

#[derive(Debug, Clone)]
pub struct DetectedFace {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub score: f32,
    #[allow(dead_code)]
    pub kps: [[f32; 2]; 5],
    pub embedding: Vec<f32>,
    pub crop: RgbaImage,
}

#[derive(Debug, Clone)]
pub struct RawDet {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub score: f32,
    pub kps: [[f32; 2]; 5],
}

pub struct FaceEngine {
    det: Mutex<Session>,
    rec: Mutex<Session>,
    det_input: String,
    det_outputs: Vec<String>,
    rec_input: String,
    rec_output: String,
}

impl FaceEngine {
    pub fn load(paths: &FaceModelPaths) -> AppResult<Self> {
        let det = load_session(&paths.det, "face-det")?;
        let rec = load_session(&paths.rec, "face-rec")?;
        let det_input = det
            .inputs()
            .first()
            .map(|i| i.name().to_string())
            .ok_or_else(|| AppError::msg("SCRFD model has no inputs"))?;
        let det_outputs: Vec<String> = det.outputs().iter().map(|o| o.name().to_string()).collect();
        if det_outputs.len() < 9 {
            return Err(AppError::msg(format!(
                "SCRFD expected ≥9 outputs, got {}",
                det_outputs.len()
            )));
        }
        let rec_input = rec
            .inputs()
            .first()
            .map(|i| i.name().to_string())
            .ok_or_else(|| AppError::msg("ArcFace model has no inputs"))?;
        let rec_output = rec
            .outputs()
            .first()
            .map(|o| o.name().to_string())
            .ok_or_else(|| AppError::msg("ArcFace model has no outputs"))?;
        Ok(Self {
            det: Mutex::new(det),
            rec: Mutex::new(rec),
            det_input,
            det_outputs,
            rec_input,
            rec_output,
        })
    }

    pub fn run_path(&self, path: &Path) -> AppResult<Vec<DetectedFace>> {
        let img = thumbnails::open_oriented(path)?;
        self.run_image(&img)
    }

    pub fn run_image(&self, img: &DynamicImage) -> AppResult<Vec<DetectedFace>> {
        let rgba = img.to_rgba8();
        let (orig_w, orig_h) = rgba.dimensions();
        if orig_w < 16 || orig_h < 16 {
            return Ok(Vec::new());
        }

        let dets = self.detect(&rgba)?;
        let mut out = Vec::new();
        for det in dets.into_iter().take(MAX_FACES) {
            let aligned = align_face(&rgba, &det.kps)?;
            let mut embedding = self.recognize(&aligned)?;
            vector::normalize(&mut embedding);
            let crop = face_crop(&rgba, det.x, det.y, det.w, det.h);
            out.push(DetectedFace {
                x: det.x,
                y: det.y,
                w: det.w,
                h: det.h,
                score: det.score,
                kps: det.kps,
                embedding,
                crop,
            });
        }
        Ok(out)
    }

    fn detect(&self, rgba: &RgbaImage) -> AppResult<Vec<RawDet>> {
        let (orig_w, orig_h) = rgba.dimensions();
        let (blob, scale) = letterbox(rgba, DET_SIZE);
        let input = Tensor::from_array(([1usize, 3, DET_SIZE as usize, DET_SIZE as usize], blob))
            .map_err(|e| AppError::msg(format!("face det tensor: {e}")))?;

        let mut session = self.det.lock();
        let outputs = session
            .run(ort::inputs![self.det_input.as_str() => input])
            .map_err(|e| AppError::msg(format!("face det inference: {e}")))?;

        let mut scored: Vec<(Vec<f32>, Vec<i64>)> = Vec::new();
        let mut boxes: Vec<(Vec<f32>, Vec<i64>)> = Vec::new();
        let mut kpss: Vec<(Vec<f32>, Vec<i64>)> = Vec::new();

        for name in &self.det_outputs {
            let value = outputs
                .get(name.as_str())
                .ok_or_else(|| AppError::msg(format!("SCRFD missing output '{name}'")))?;
            let (shape, data) = value
                .try_extract_tensor::<f32>()
                .map_err(|e| AppError::msg(format!("SCRFD extract {name}: {e}")))?;
            let shape: Vec<i64> = shape.iter().copied().collect();
            let data = data.to_vec();
            let last = *shape.last().unwrap_or(&1);
            match last {
                1 => scored.push((data, shape)),
                4 => boxes.push((data, shape)),
                10 => kpss.push((data, shape)),
                _ => {}
            }
        }

        if scored.len() != 3 || boxes.len() != 3 || kpss.len() != 3 {
            return Err(AppError::msg(format!(
                "SCRFD output groups incomplete: scores={} boxes={} kps={}",
                scored.len(),
                boxes.len(),
                kpss.len()
            )));
        }

        let order = |shape: &[i64]| -> i64 {
            shape.iter().product::<i64>() / shape.last().copied().unwrap_or(1).max(1)
        };
        scored.sort_by_key(|(_, s)| std::cmp::Reverse(order(s)));
        boxes.sort_by_key(|(_, s)| std::cmp::Reverse(order(s)));
        kpss.sort_by_key(|(_, s)| std::cmp::Reverse(order(s)));

        let mut candidates = Vec::new();
        for (i, stride) in STRIDES.iter().enumerate() {
            let (ref scores, ref score_shape) = scored[i];
            let (ref dists, _) = boxes[i];
            let (ref kps_data, _) = kpss[i];
            let n = score_shape.iter().product::<i64>() as usize;
            let feat = (DET_SIZE / stride) as usize;
            let expected = feat * feat * NUM_ANCHORS;
            if n < expected || dists.len() < expected * 4 || kps_data.len() < expected * 10 {
                continue;
            }
            for idx in 0..expected {
                let score = scores[idx];
                if score < DET_THRESH {
                    continue;
                }
                let ay = ((idx / NUM_ANCHORS) / feat) as f32;
                let ax = ((idx / NUM_ANCHORS) % feat) as f32;
                let cx = (ax + 0.5) * *stride as f32;
                let cy = (ay + 0.5) * *stride as f32;
                let d = &dists[idx * 4..idx * 4 + 4];
                let (x, y, w, h) = distance2bbox(cx, cy, [d[0], d[1], d[2], d[3]], *stride as f32);
                let x = (x / scale).clamp(0.0, orig_w as f32 - 1.0);
                let y = (y / scale).clamp(0.0, orig_h as f32 - 1.0);
                let w = (w / scale).max(1.0).min(orig_w as f32 - x);
                let h = (h / scale).max(1.0).min(orig_h as f32 - y);
                let mut kps = [[0.0f32; 2]; 5];
                for k in 0..5 {
                    let kx = kps_data[idx * 10 + k * 2] * *stride as f32 + cx;
                    let ky = kps_data[idx * 10 + k * 2 + 1] * *stride as f32 + cy;
                    kps[k] = [kx / scale, ky / scale];
                }
                candidates.push(RawDet {
                    x,
                    y,
                    w,
                    h,
                    score,
                    kps,
                });
            }
        }

        Ok(nms(candidates, NMS_THRESH))
    }

    fn recognize(&self, aligned: &RgbaImage) -> AppResult<Vec<f32>> {
        let pixels = arcface_pixels(aligned);
        let input =
            Tensor::from_array(([1usize, 3, REC_SIZE as usize, REC_SIZE as usize], pixels))
                .map_err(|e| AppError::msg(format!("face rec tensor: {e}")))?;
        let mut session = self.rec.lock();
        let outputs = session
            .run(ort::inputs![self.rec_input.as_str() => input])
            .map_err(|e| AppError::msg(format!("face rec inference: {e}")))?;
        let value = outputs
            .get(self.rec_output.as_str())
            .ok_or_else(|| AppError::msg(format!("ArcFace missing '{}'", self.rec_output)))?;
        let (shape, data) = value
            .try_extract_tensor::<f32>()
            .map_err(|e| AppError::msg(format!("ArcFace extract: {e}")))?;
        let n: usize = shape.iter().map(|d| *d as usize).product();
        if n < 512 {
            return Err(AppError::msg(format!(
                "ArcFace embedding too short: shape={shape:?}"
            )));
        }
        Ok(data[..512].to_vec())
    }
}

fn load_session(path: &Path, label: &str) -> AppResult<Session> {
    Session::builder()
        .map_err(|e| AppError::msg(format!("ort session builder ({label}): {e}")))?
        .commit_from_file(path)
        .map_err(|e| {
            AppError::msg(format!(
                "failed to load {label} from {}: {e}",
                path.display()
            ))
        })
}

fn letterbox(rgba: &RgbaImage, size: u32) -> (Vec<f32>, f32) {
    let (ow, oh) = rgba.dimensions();
    let scale = (size as f32 / ow as f32).min(size as f32 / oh as f32);
    let nw = (ow as f32 * scale).round().max(1.0) as u32;
    let nh = (oh as f32 * scale).round().max(1.0) as u32;
    let resized = imageops::resize(rgba, nw, nh, FilterType::Triangle);
    let mut canvas = RgbaImage::from_pixel(size, size, Rgba([0, 0, 0, 255]));
    imageops::replace(&mut canvas, &resized, 0, 0);

    let mut out = vec![0.0f32; (3 * size * size) as usize];
    let plane = (size * size) as usize;
    for y in 0..size {
        for x in 0..size {
            let p = canvas.get_pixel(x, y).0;
            let i = (y * size + x) as usize;
            out[i] = (p[0] as f32 - 127.5) / 128.0;
            out[plane + i] = (p[1] as f32 - 127.5) / 128.0;
            out[2 * plane + i] = (p[2] as f32 - 127.5) / 128.0;
        }
    }
    (out, scale)
}

fn arcface_pixels(aligned: &RgbaImage) -> Vec<f32> {
    let mut out = vec![0.0f32; (3 * REC_SIZE * REC_SIZE) as usize];
    let plane = (REC_SIZE * REC_SIZE) as usize;
    for y in 0..REC_SIZE {
        for x in 0..REC_SIZE {
            let p = aligned.get_pixel(x, y).0;
            let i = (y * REC_SIZE + x) as usize;
            out[i] = (p[0] as f32 - 127.5) / 127.5;
            out[plane + i] = (p[1] as f32 - 127.5) / 127.5;
            out[2 * plane + i] = (p[2] as f32 - 127.5) / 127.5;
        }
    }
    out
}

fn face_crop(rgba: &RgbaImage, x: f32, y: f32, w: f32, h: f32) -> RgbaImage {
    let (iw, ih) = rgba.dimensions();
    let x0 = x.floor().max(0.0) as u32;
    let y0 = y.floor().max(0.0) as u32;
    let x1 = (x + w).ceil().min(iw as f32) as u32;
    let y1 = (y + h).ceil().min(ih as f32) as u32;
    let cw = x1.saturating_sub(x0).max(1);
    let ch = y1.saturating_sub(y0).max(1);
    let cropped = imageops::crop_imm(rgba, x0, y0, cw, ch).to_image();
    imageops::resize(&cropped, 160, 160, FilterType::Triangle)
}

pub fn nms(mut dets: Vec<RawDet>, iou_thresh: f32) -> Vec<RawDet> {
    dets.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let mut keep = Vec::new();
    let mut suppressed = vec![false; dets.len()];
    for i in 0..dets.len() {
        if suppressed[i] {
            continue;
        }
        keep.push(dets[i].clone());
        for j in (i + 1)..dets.len() {
            if !suppressed[j] && iou(&dets[i], &dets[j]) > iou_thresh {
                suppressed[j] = true;
            }
        }
    }
    keep
}

pub fn iou(a: &RawDet, b: &RawDet) -> f32 {
    let ax2 = a.x + a.w;
    let ay2 = a.y + a.h;
    let bx2 = b.x + b.w;
    let by2 = b.y + b.h;
    let ix1 = a.x.max(b.x);
    let iy1 = a.y.max(b.y);
    let ix2 = ax2.min(bx2);
    let iy2 = ay2.min(by2);
    let iw = (ix2 - ix1).max(0.0);
    let ih = (iy2 - iy1).max(0.0);
    let inter = iw * ih;
    let uni = a.w * a.h + b.w * b.h - inter;
    if uni <= 0.0 {
        0.0
    } else {
        inter / uni
    }
}

pub fn distance2bbox(cx: f32, cy: f32, dist: [f32; 4], stride: f32) -> (f32, f32, f32, f32) {
    let x1 = cx - dist[0] * stride;
    let y1 = cy - dist[1] * stride;
    let x2 = cx + dist[2] * stride;
    let y2 = cy + dist[3] * stride;
    (x1, y1, x2 - x1, y2 - y1)
}

/// Similarity-align source keypoints to the ArcFace template, then warp to 112×112.
pub fn align_face(rgba: &RgbaImage, kps: &[[f32; 2]; 5]) -> AppResult<RgbaImage> {
    let (a, b, tx, ty) = umeyama(kps, &ARC_FACE_REF)?;
    let mut out: RgbaImage = ImageBuffer::from_pixel(REC_SIZE, REC_SIZE, Rgba([0, 0, 0, 255]));
    let (iw, ih) = rgba.dimensions();
    for y in 0..REC_SIZE {
        for x in 0..REC_SIZE {
            let dx = x as f32 - tx;
            let dy = y as f32 - ty;
            let det = a * a + b * b;
            if det.abs() < 1e-8 {
                continue;
            }
            // Inverse of [[a,-b],[b,a]]
            let src_x = (a * dx + b * dy) / det;
            let src_y = (-b * dx + a * dy) / det;
            if src_x < 0.0 || src_y < 0.0 || src_x >= iw as f32 - 1.0 || src_y >= ih as f32 - 1.0 {
                continue;
            }
            out.put_pixel(x, y, bilinear(rgba, src_x, src_y));
        }
    }
    Ok(out)
}

/// Estimate similarity transform mapping `src` → `dst`.
/// Returns `(a, b, tx, ty)` for `[[a,-b],[b,a]] * src + [tx, ty]`.
pub fn umeyama(src: &[[f32; 2]; 5], dst: &[[f32; 2]; 5]) -> AppResult<(f32, f32, f32, f32)> {
    let n = 5.0f32;
    let mut src_mean = [0.0f32; 2];
    let mut dst_mean = [0.0f32; 2];
    for i in 0..5 {
        src_mean[0] += src[i][0];
        src_mean[1] += src[i][1];
        dst_mean[0] += dst[i][0];
        dst_mean[1] += dst[i][1];
    }
    src_mean[0] /= n;
    src_mean[1] /= n;
    dst_mean[0] /= n;
    dst_mean[1] /= n;

    let mut src_var = 0.0f32;
    // Cross-covariance in complex form for 2D similarity:
    // Let src' = sx + i sy, dst' = dx + i dy (centred).
    // scale*e^{iθ} = sum(conj(src') * dst') / sum(|src'|^2)
    let mut real = 0.0f32;
    let mut imag = 0.0f32;
    for i in 0..5 {
        let sx = src[i][0] - src_mean[0];
        let sy = src[i][1] - src_mean[1];
        let dx = dst[i][0] - dst_mean[0];
        let dy = dst[i][1] - dst_mean[1];
        src_var += sx * sx + sy * sy;
        // conj(sx+isy)*(dx+idy) = (sx-isy)(dx+idy) = sx*dx + sy*dy + i(sx*dy - sy*dx)
        real += sx * dx + sy * dy;
        imag += sx * dy - sy * dx;
    }
    if src_var < 1e-8 {
        return Ok((1.0, 0.0, dst_mean[0] - src_mean[0], dst_mean[1] - src_mean[1]));
    }
    let a = real / src_var;
    let b = imag / src_var;
    let tx = dst_mean[0] - (a * src_mean[0] - b * src_mean[1]);
    let ty = dst_mean[1] - (b * src_mean[0] + a * src_mean[1]);
    Ok((a, b, tx, ty))
}

fn bilinear(img: &RgbaImage, x: f32, y: f32) -> Rgba<u8> {
    let x0 = x.floor() as u32;
    let y0 = y.floor() as u32;
    let x1 = x0 + 1;
    let y1 = y0 + 1;
    let (w, h) = img.dimensions();
    if x1 >= w || y1 >= h {
        return *img.get_pixel(x0.min(w - 1), y0.min(h - 1));
    }
    let dx = x - x0 as f32;
    let dy = y - y0 as f32;
    let p00 = img.get_pixel(x0, y0).0;
    let p10 = img.get_pixel(x1, y0).0;
    let p01 = img.get_pixel(x0, y1).0;
    let p11 = img.get_pixel(x1, y1).0;
    let mut out = [0u8; 4];
    for c in 0..4 {
        let v = (1.0 - dx) * (1.0 - dy) * p00[c] as f32
            + dx * (1.0 - dy) * p10[c] as f32
            + (1.0 - dx) * dy * p01[c] as f32
            + dx * dy * p11[c] as f32;
        out[c] = v.round().clamp(0.0, 255.0) as u8;
    }
    Rgba(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distance2bbox_expands_from_center() {
        let (x, y, w, h) = distance2bbox(100.0, 100.0, [1.0, 2.0, 3.0, 4.0], 10.0);
        assert!((x - 90.0).abs() < 1e-5);
        assert!((y - 80.0).abs() < 1e-5);
        assert!((w - 40.0).abs() < 1e-5);
        assert!((h - 60.0).abs() < 1e-5);
    }

    #[test]
    fn nms_keeps_highest_of_overlapping() {
        let a = RawDet {
            x: 0.0,
            y: 0.0,
            w: 10.0,
            h: 10.0,
            score: 0.9,
            kps: [[0.0; 2]; 5],
        };
        let b = RawDet {
            x: 1.0,
            y: 1.0,
            w: 10.0,
            h: 10.0,
            score: 0.5,
            kps: [[0.0; 2]; 5],
        };
        let c = RawDet {
            x: 50.0,
            y: 50.0,
            w: 10.0,
            h: 10.0,
            score: 0.8,
            kps: [[0.0; 2]; 5],
        };
        let kept = nms(vec![a, b, c], 0.3);
        assert_eq!(kept.len(), 2);
        assert!((kept[0].score - 0.9).abs() < 1e-5);
        assert!((kept[1].score - 0.8).abs() < 1e-5);
    }

    #[test]
    fn umeyama_identity_on_template() {
        let (a, b, tx, ty) = umeyama(&ARC_FACE_REF, &ARC_FACE_REF).unwrap();
        assert!((a - 1.0).abs() < 1e-3, "a={a}");
        assert!(b.abs() < 1e-3, "b={b}");
        assert!(tx.abs() < 1e-2, "tx={tx}");
        assert!(ty.abs() < 1e-2, "ty={ty}");
    }

    #[test]
    fn umeyama_recovers_translation() {
        let mut src = ARC_FACE_REF;
        for p in &mut src {
            p[0] += 10.0;
            p[1] += 5.0;
        }
        let (a, b, tx, ty) = umeyama(&src, &ARC_FACE_REF).unwrap();
        assert!((a - 1.0).abs() < 1e-3);
        assert!(b.abs() < 1e-3);
        assert!((tx + 10.0).abs() < 0.5, "tx={tx}");
        assert!((ty + 5.0).abs() < 0.5, "ty={ty}");
    }
}
