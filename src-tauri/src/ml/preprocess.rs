//! CLIP image preprocessing matching HuggingFace `CLIPImageProcessor`.
//!
//! Pipeline: RGB → resize shortest edge to 224 (bicubic) → center crop 224×224
//! → scale to [0,1] → channel-wise CLIP mean/std → NCHW float32.

use image::{imageops, DynamicImage};

use crate::error::{AppError, AppResult};

pub const IMAGE_SIZE: u32 = 224;
// CLIP mean/std from HuggingFace CLIPImageProcessor, truncated to f32 precision.
pub const IMAGE_MEAN: [f32; 3] = [0.48145466, 0.4578275, 0.40821073];
pub const IMAGE_STD: [f32; 3] = [0.26862954, 0.261_302_6, 0.275_777_1];

/// Produce a single-image `pixel_values` buffer shaped `[1, 3, 224, 224]`.
pub fn pixel_values(img: &DynamicImage) -> Vec<f32> {
    let rgb = img.to_rgb8();
    let (w, h) = rgb.dimensions();
    // Match transformers: scale so the shortest edge becomes IMAGE_SIZE.
    let (nw, nh) = if w <= h {
        let scale = IMAGE_SIZE as f32 / w as f32;
        (IMAGE_SIZE, (h as f32 * scale).round().max(1.0) as u32)
    } else {
        let scale = IMAGE_SIZE as f32 / h as f32;
        ((w as f32 * scale).round().max(1.0) as u32, IMAGE_SIZE)
    };
    // Bicubic ≈ PIL BICUBIC (resample=3 in the preprocessor config).
    let resized = imageops::resize(&rgb, nw, nh, imageops::FilterType::CatmullRom);
    let left = nw.saturating_sub(IMAGE_SIZE) / 2;
    let top = nh.saturating_sub(IMAGE_SIZE) / 2;
    let cropped = imageops::crop_imm(&resized, left, top, IMAGE_SIZE, IMAGE_SIZE).to_image();

    let mut out = vec![0.0f32; (3 * IMAGE_SIZE * IMAGE_SIZE) as usize];
    let plane = (IMAGE_SIZE * IMAGE_SIZE) as usize;
    for y in 0..IMAGE_SIZE {
        for x in 0..IMAGE_SIZE {
            let p = cropped.get_pixel(x, y).0;
            let idx = (y * IMAGE_SIZE + x) as usize;
            for c in 0..3 {
                let v = p[c] as f32 * (1.0 / 255.0);
                out[c * plane + idx] = (v - IMAGE_MEAN[c]) / IMAGE_STD[c];
            }
        }
    }
    out
}

pub fn pixel_values_from_path(path: &std::path::Path) -> AppResult<Vec<f32>> {
    let img = crate::thumbnails::open_oriented(path).map_err(|e| {
        AppError::msg(format!("failed to open image for embedding ({}): {e}", path.display()))
    })?;
    Ok(pixel_values(&img))
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgb, RgbImage};

    #[test]
    fn output_is_nchw_224() {
        let img = DynamicImage::ImageRgb8(RgbImage::from_pixel(400, 300, Rgb([128, 64, 32])));
        let v = pixel_values(&img);
        assert_eq!(v.len(), 3 * 224 * 224);
        assert!(v.iter().all(|x| x.is_finite()), "no NaNs from preprocess");
    }

    #[test]
    fn portrait_and_landscape_both_crop_to_square() {
        let wide = DynamicImage::ImageRgb8(RgbImage::from_pixel(800, 200, Rgb([10, 20, 30])));
        let tall = DynamicImage::ImageRgb8(RgbImage::from_pixel(200, 800, Rgb([10, 20, 30])));
        assert_eq!(pixel_values(&wide).len(), pixel_values(&tall).len());
    }

    #[test]
    fn known_mean_pixel_lands_near_zero_after_normalize() {
        // A pixel at the CLIP mean should sit near 0 after (x - mean) / std.
        let mean_rgb = Rgb([
            (IMAGE_MEAN[0] * 255.0).round() as u8,
            (IMAGE_MEAN[1] * 255.0).round() as u8,
            (IMAGE_MEAN[2] * 255.0).round() as u8,
        ]);
        let img = DynamicImage::ImageRgb8(RgbImage::from_pixel(256, 256, mean_rgb));
        let v = pixel_values(&img);
        let plane = 224 * 224;
        for c in 0..3 {
            let avg: f32 = v[c * plane..(c + 1) * plane].iter().sum::<f32>() / plane as f32;
            assert!(
                avg.abs() < 0.05,
                "channel {c} average {avg} should be near 0 for a mean-coloured image"
            );
        }
    }
}
