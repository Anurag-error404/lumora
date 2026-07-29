//! Florence-2 greedy caption decoding through its split ONNX graph.

use std::path::Path;

use image::imageops::FilterType;
use image::DynamicImage;
use ort::session::Session;
use ort::value::Tensor;
use parking_lot::Mutex;
use tokenizers::Tokenizer;

use crate::captions::CaptionsModelPaths;
use crate::error::{AppError, AppResult};
use crate::thumbnails;

const IMAGE_SIZE: usize = 768;
const MEAN: [f32; 3] = [0.485, 0.456, 0.406];
const STD: [f32; 3] = [0.229, 0.224, 0.225];
/// BART-family decoder start / EOS for Florence-2 (`generation_config.json`).
const DECODER_START_TOKEN_ID: i64 = 2;
const EOS_TOKEN_ID: i64 = 2;
const MAX_NEW_TOKENS: usize = 64;
/// Florence-2 processor maps task token `<CAPTION>` to this natural-language prompt.
/// Encoding the literal `<CAPTION>` string falls apart into BPE pieces and yields junk.
const CAPTION_PROMPT: &str = "What does the image describe?";

pub struct CaptionsEngine {
    vision: Mutex<Session>,
    embed: Mutex<Session>,
    encoder: Mutex<Session>,
    decoder: Mutex<Session>,
    tokenizer: Tokenizer,
}

impl CaptionsEngine {
    pub fn load(paths: &CaptionsModelPaths) -> AppResult<Self> {
        Ok(Self {
            vision: Mutex::new(crate::ml::session::load_session(
                &paths.vision,
                "Florence-2 vision",
            )?),
            embed: Mutex::new(crate::ml::session::load_session(
                &paths.embed,
                "Florence-2 embeddings",
            )?),
            encoder: Mutex::new(crate::ml::session::load_session(
                &paths.encoder,
                "Florence-2 encoder",
            )?),
            decoder: Mutex::new(crate::ml::session::load_session(
                &paths.decoder,
                "Florence-2 decoder",
            )?),
            tokenizer: Tokenizer::from_file(&paths.tokenizer)
                .map_err(|e| AppError::msg(format!("load Florence-2 tokenizer: {e}")))?,
        })
    }

    pub fn run_path(&self, path: &Path) -> AppResult<String> {
        self.run_image(&thumbnails::open_oriented(path)?)
    }

    pub fn run_image(&self, image: &DynamicImage) -> AppResult<String> {
        let prompt = self
            .tokenizer
            .encode(CAPTION_PROMPT, true)
            .map_err(|e| AppError::msg(format!("tokenize Florence-2 prompt: {e}")))?;
        let prompt_ids: Vec<i64> = prompt.get_ids().iter().map(|&id| i64::from(id)).collect();
        if prompt_ids.is_empty() {
            return Err(AppError::msg(
                "Florence-2 tokenizer returned an empty prompt",
            ));
        }

        let (image_features, image_tokens, hidden) = self.run_vision(image)?;
        let (text_embeds, text_tokens, text_hidden) = self.run_embed(&prompt_ids)?;
        if hidden != text_hidden {
            return Err(AppError::msg(format!(
                "Florence-2 incompatible embedding dims: vision {hidden}, text {text_hidden}"
            )));
        }

        let mut inputs_embeds = image_features;
        inputs_embeds.extend_from_slice(&text_embeds);
        let sequence_len = image_tokens + text_tokens;
        let encoded = self.run_encoder(&inputs_embeds, sequence_len, hidden)?;

        let mut decoder_ids = vec![DECODER_START_TOKEN_ID];
        let mut generated: Vec<u32> = Vec::new();
        for _ in 0..MAX_NEW_TOKENS {
            let next = self.run_decoder(&encoded, sequence_len, hidden, &decoder_ids)?;
            if next == EOS_TOKEN_ID {
                break;
            }
            generated.push(next as u32);
            decoder_ids.push(next);
        }

        // Match Florence-2 post_process for pure_text: keep special markers out of
        // the final string, but do not skip them during decode (they delimit tasks).
        let decoded = self
            .tokenizer
            .decode(&generated, false)
            .map_err(|e| AppError::msg(format!("decode Florence-2 output: {e}")))?;
        let caption = clean_caption(&decoded);
        if caption.is_empty() {
            return Err(AppError::msg("Florence-2 produced an empty caption"));
        }
        Ok(caption)
    }

    fn run_vision(&self, image: &DynamicImage) -> AppResult<(Vec<f32>, usize, usize)> {
        let pixels = preprocess(image);
        let input = Tensor::from_array(([1usize, 3, IMAGE_SIZE, IMAGE_SIZE], pixels))
            .map_err(|e| AppError::msg(format!("Florence-2 vision tensor: {e}")))?;
        let mut session = self.vision.lock();
        let input_name = input_name(&session, &["pixel_values"])?;
        let output_name = output_name(&session, &["image_features", "last_hidden_state"])?;
        let outputs = session
            .run(ort::inputs![input_name.as_str() => input])
            .map_err(|e| AppError::msg(format!("Florence-2 vision inference: {e}")))?;
        let (data, seq, hidden) = output_seq_hidden(&outputs, &output_name, "vision")?;
        Ok((data, seq, hidden))
    }

    fn run_embed(&self, ids: &[i64]) -> AppResult<(Vec<f32>, usize, usize)> {
        let input = Tensor::from_array(([1usize, ids.len()], ids.to_vec()))
            .map_err(|e| AppError::msg(format!("Florence-2 prompt tensor: {e}")))?;
        let mut session = self.embed.lock();
        let input_name = input_name(&session, &["input_ids"])?;
        let output_name = output_name(&session, &["inputs_embeds", "last_hidden_state"])?;
        let outputs = session
            .run(ort::inputs![input_name.as_str() => input])
            .map_err(|e| AppError::msg(format!("Florence-2 embedding inference: {e}")))?;
        let (data, seq, hidden) = output_seq_hidden(&outputs, &output_name, "token embeddings")?;
        Ok((data, seq, hidden))
    }

    fn run_encoder(&self, embeds: &[f32], seq: usize, hidden: usize) -> AppResult<Vec<f32>> {
        let values = Tensor::from_array(([1usize, seq, hidden], embeds.to_vec()))
            .map_err(|e| AppError::msg(format!("Florence-2 encoder embeddings tensor: {e}")))?;
        let mask = Tensor::from_array(([1usize, seq], vec![1i64; seq]))
            .map_err(|e| AppError::msg(format!("Florence-2 encoder mask tensor: {e}")))?;
        let mut session = self.encoder.lock();
        let embeds_name = require_input_name(&session, &["inputs_embeds"])?;
        let mask_name = require_input_name(&session, &["attention_mask"])?;
        let output_name = output_name(&session, &["last_hidden_state", "encoder_hidden_states"])?;
        let outputs = session
            .run(ort::inputs![
                embeds_name.as_str() => values,
                mask_name.as_str() => mask
            ])
            .map_err(|e| AppError::msg(format!("Florence-2 encoder inference: {e}")))?;
        let (data, out_seq, out_hidden) = output_seq_hidden(&outputs, &output_name, "encoder")?;
        if out_seq != seq || out_hidden != hidden {
            return Err(AppError::msg(format!(
                "Florence-2 encoder shape mismatch: expected [1,{seq},{hidden}], got [1,{out_seq},{out_hidden}]"
            )));
        }
        Ok(data)
    }

    fn run_decoder(
        &self,
        encoded: &[f32],
        seq: usize,
        hidden: usize,
        ids: &[i64],
    ) -> AppResult<i64> {
        // onnx-community Florence decoder takes `inputs_embeds`, not token ids —
        // look up embeddings via the shared embed_tokens session first.
        let (decoder_embeds, dec_seq, dec_hidden) = self.run_embed(ids)?;
        if dec_seq != ids.len() || dec_hidden != hidden {
            return Err(AppError::msg(format!(
                "Florence-2 decoder embeds shape mismatch: got [1,{dec_seq},{dec_hidden}], expected [1,{},{hidden}]",
                ids.len()
            )));
        }

        let states = Tensor::from_array(([1usize, seq, hidden], encoded.to_vec()))
            .map_err(|e| AppError::msg(format!("Florence-2 decoder states tensor: {e}")))?;
        let mask = Tensor::from_array(([1usize, seq], vec![1i64; seq]))
            .map_err(|e| AppError::msg(format!("Florence-2 decoder mask tensor: {e}")))?;
        let embeds = Tensor::from_array(([1usize, dec_seq, hidden], decoder_embeds))
            .map_err(|e| AppError::msg(format!("Florence-2 decoder embeds tensor: {e}")))?;
        let mut session = self.decoder.lock();
        let states_name = require_input_name(&session, &["encoder_hidden_states"])?;
        let mask_name =
            require_input_name(&session, &["encoder_attention_mask", "attention_mask"])?;
        let embeds_name = require_input_name(&session, &["inputs_embeds", "decoder_inputs_embeds"])?;
        let output_name = output_name(&session, &["logits"])?;
        let outputs = session
            .run(ort::inputs![
                states_name.as_str() => states,
                mask_name.as_str() => mask,
                embeds_name.as_str() => embeds
            ])
            .map_err(|e| AppError::msg(format!("Florence-2 decoder inference: {e}")))?;
        let (logits, steps, vocab) = output_seq_hidden(&outputs, &output_name, "decoder")?;
        if steps != ids.len() {
            return Err(AppError::msg(format!(
                "Florence-2 decoder steps {steps} != input length {}",
                ids.len()
            )));
        }
        let last = logits
            .get(logits.len().saturating_sub(vocab)..)
            .ok_or_else(|| AppError::msg("Florence-2 decoder returned empty logits"))?;
        last.iter()
            .enumerate()
            .max_by(|a, b| a.1.partial_cmp(b.1).unwrap_or(std::cmp::Ordering::Equal))
            .map(|(id, _)| id as i64)
            .ok_or_else(|| AppError::msg("Florence-2 decoder has no vocabulary logits"))
    }
}

fn input_name(session: &Session, preferred: &[&str]) -> AppResult<String> {
    preferred
        .iter()
        .find_map(|wanted| {
            session
                .inputs()
                .iter()
                .find(|input| input.name() == *wanted)
        })
        .or_else(|| session.inputs().first())
        .map(|input| input.name().to_string())
        .ok_or_else(|| {
            AppError::msg(format!(
                "Florence-2 session has no input (expected {})",
                preferred.join(" or ")
            ))
        })
}

/// Like [`input_name`], but never silently falls back to an unrelated first input.
fn require_input_name(session: &Session, preferred: &[&str]) -> AppResult<String> {
    preferred
        .iter()
        .find_map(|wanted| {
            session
                .inputs()
                .iter()
                .find(|input| input.name() == *wanted)
                .map(|input| input.name().to_string())
        })
        .ok_or_else(|| {
            let available: Vec<_> = session.inputs().iter().map(|i| i.name().to_string()).collect();
            AppError::msg(format!(
                "Florence-2 missing input {} (have: {})",
                preferred.join(" or "),
                available.join(", ")
            ))
        })
}

fn output_name(session: &Session, preferred: &[&str]) -> AppResult<String> {
    preferred
        .iter()
        .find_map(|wanted| {
            session
                .outputs()
                .iter()
                .find(|output| output.name() == *wanted)
        })
        .or_else(|| session.outputs().first())
        .map(|output| output.name().to_string())
        .ok_or_else(|| {
            AppError::msg(format!(
                "Florence-2 session has no output (expected {})",
                preferred.join(" or ")
            ))
        })
}

/// Extract a rank-3 `[1, seq, hidden]` (or logits `[1, steps, vocab]`) tensor.
fn output_seq_hidden(
    outputs: &ort::session::SessionOutputs<'_>,
    name: &str,
    stage: &str,
) -> AppResult<(Vec<f32>, usize, usize)> {
    let value = outputs
        .get(name)
        .ok_or_else(|| AppError::msg(format!("Florence-2 {stage} missing output '{name}'")))?;
    let (shape, data) = value
        .try_extract_tensor::<f32>()
        .map_err(|e| AppError::msg(format!("Florence-2 {stage} output '{name}': {e}")))?;
    if shape.len() != 3 || shape[0] != 1 {
        return Err(AppError::msg(format!(
            "Florence-2 {stage} expected shape [1,seq,dim], got {shape:?}"
        )));
    }
    let seq = shape[1] as usize;
    let hidden = shape[2] as usize;
    let values: Vec<f32> = data.iter().copied().collect();
    if values.len() != seq * hidden {
        return Err(AppError::msg(format!(
            "Florence-2 {stage} length {} != {seq}×{hidden}",
            values.len()
        )));
    }
    Ok((values, seq, hidden))
}

fn preprocess(image: &DynamicImage) -> Vec<f32> {
    let resized = image.to_rgb8();
    let resized = image::imageops::resize(
        &resized,
        IMAGE_SIZE as u32,
        IMAGE_SIZE as u32,
        FilterType::Triangle,
    );
    let mut out = vec![0.0; 3 * IMAGE_SIZE * IMAGE_SIZE];
    let plane = IMAGE_SIZE * IMAGE_SIZE;
    for y in 0..IMAGE_SIZE {
        for x in 0..IMAGE_SIZE {
            let pixel = resized.get_pixel(x as u32, y as u32).0;
            let index = y * IMAGE_SIZE + x;
            for channel in 0..3 {
                out[channel * plane + index] =
                    (pixel[channel] as f32 / 255.0 - MEAN[channel]) / STD[channel];
            }
        }
    }
    out
}

fn clean_caption(raw: &str) -> String {
    let mut text = raw
        .replace("<s>", "")
        .replace("</s>", "")
        .replace("<pad>", "")
        .replace("<unk>", "");
    for prefix in [
        "<CAPTION>",
        "<DETAILED_CAPTION>",
        "<MORE_DETAILED_CAPTION>",
        "CAPTION",
    ] {
        if let Some(rest) = text.trim().strip_prefix(prefix) {
            text = rest.to_string();
        }
    }
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn installed_models() -> Option<CaptionsModelPaths> {
        let home = std::env::var_os("HOME").map(PathBuf::from)?;
        let dir = home.join("Library/Application Support/com.photovault.ai/models");
        let paths = CaptionsModelPaths {
            vision: dir.join("florence2-vision.onnx"),
            embed: dir.join("florence2-embed.onnx"),
            encoder: dir.join("florence2-encoder.onnx"),
            decoder: dir.join("florence2-decoder.onnx"),
            tokenizer: dir.join("florence2-tokenizer.json"),
        };
        if [
            &paths.vision,
            &paths.embed,
            &paths.encoder,
            &paths.decoder,
            &paths.tokenizer,
        ]
        .iter()
        .all(|p| p.exists())
        {
            Some(paths)
        } else {
            None
        }
    }

    #[test]
    fn decoder_session_expects_inputs_embeds() {
        let Some(paths) = installed_models() else {
            eprintln!("skip: Florence-2 models not installed locally");
            return;
        };
        let session = crate::ml::session::load_session(&paths.decoder, "decoder").unwrap();
        let names: Vec<_> = session.inputs().iter().map(|i| i.name().to_string()).collect();
        assert!(
            names.iter().any(|n| n == "inputs_embeds"),
            "decoder inputs: {names:?}"
        );
        assert!(
            !names
                .iter()
                .any(|n| n == "input_ids" || n == "decoder_input_ids"),
            "unexpected id inputs: {names:?}"
        );
    }

    #[test]
    fn clean_caption_strips_task_junk() {
        assert_eq!(clean_caption("<s>a red bike</s>"), "a red bike");
        assert_eq!(clean_caption("CAPTION"), "");
        assert_eq!(clean_caption("<CAPTION> a lake at dusk"), "a lake at dusk");
    }

    #[test]
    fn caption_prompt_encodes_as_natural_language() {
        let Some(paths) = installed_models() else {
            eprintln!("skip: Florence-2 models not installed locally");
            return;
        };
        let tok = Tokenizer::from_file(&paths.tokenizer).unwrap();
        let enc = tok.encode(CAPTION_PROMPT, true).unwrap();
        let tokens = enc.get_tokens();
        assert!(
            !tokens.iter().any(|t| t == "CAP" || t == "TION"),
            "prompt should not BPE-split CAPTION; got {tokens:?}"
        );
        assert!(tokens.len() > 3, "expected a full sentence prompt, got {tokens:?}");
    }
}
