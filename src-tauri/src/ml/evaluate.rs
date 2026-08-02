//! Probe local ONNX files against capability profiles before import.

use std::path::Path;

use ort::value::Tensor;
use serde::{Deserialize, Serialize};

use super::profiles::{
    self, ProfileKind, AUTOTAGS_INPUT_SIZES, AUTOTAGS_LABEL_COUNT, CLIP_ATTENTION_MASK,
    CLIP_EMBED_DIM, CLIP_IMAGE_EMBEDS, CLIP_IMAGE_SIZE, CLIP_INPUT_IDS, CLIP_PIXEL_VALUES,
    CLIP_SEQ_LEN, CLIP_TEXT_EMBEDS,
};
use super::session;
use crate::error::{AppError, AppResult};
use crate::ml::library::Capability;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvalReport {
    pub compatible: bool,
    pub capability: String,
    pub profile: String,
    pub reasons: Vec<String>,
    pub warnings: Vec<String>,
    pub input_size: Option<u32>,
    pub embedding_dim: Option<usize>,
    pub input_names: Vec<String>,
    pub output_names: Vec<String>,
}

fn ok_report(
    capability: Capability,
    profile: ProfileKind,
    reasons: Vec<String>,
    warnings: Vec<String>,
    input_size: Option<u32>,
    embedding_dim: Option<usize>,
    input_names: Vec<String>,
    output_names: Vec<String>,
) -> EvalReport {
    EvalReport {
        compatible: true,
        capability: capability.as_str().to_string(),
        profile: profile.as_str().to_string(),
        reasons,
        warnings,
        input_size,
        embedding_dim,
        input_names,
        output_names,
    }
}

fn bad_report(
    capability: Capability,
    profile: ProfileKind,
    reasons: Vec<String>,
    input_names: Vec<String>,
    output_names: Vec<String>,
) -> EvalReport {
    EvalReport {
        compatible: false,
        capability: capability.as_str().to_string(),
        profile: profile.as_str().to_string(),
        reasons,
        warnings: Vec::new(),
        input_size: None,
        embedding_dim: None,
        input_names,
        output_names,
    }
}

fn session_io_names(path: &Path, label: &str) -> AppResult<(Vec<String>, Vec<String>, ort::session::Session)> {
    let session = session::load_session(path, label)?;
    let inputs: Vec<String> = session.inputs().iter().map(|i| i.name().to_string()).collect();
    let outputs: Vec<String> = session.outputs().iter().map(|o| o.name().to_string()).collect();
    Ok((inputs, outputs, session))
}

fn load_label_count(path: &Path) -> AppResult<usize> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| AppError::msg(format!("read labels: {e}")))?;
    let n = text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .count();
    Ok(n)
}

/// Evaluate a local ImageNet-style classifier (+ labels file).
pub fn evaluate_autotags(model: &Path, labels: &Path) -> AppResult<EvalReport> {
    let capability = Capability::AutoTags;
    let profile = ProfileKind::AutoTags;
    if !model.is_file() {
        return Ok(bad_report(
            capability,
            profile,
            vec!["model file not found".into()],
            vec![],
            vec![],
        ));
    }
    if !labels.is_file() {
        return Ok(bad_report(
            capability,
            profile,
            vec!["labels .txt file not found".into()],
            vec![],
            vec![],
        ));
    }

    let label_count = load_label_count(labels)?;
    if label_count != AUTOTAGS_LABEL_COUNT {
        return Ok(bad_report(
            capability,
            profile,
            vec![format!(
                "expected {AUTOTAGS_LABEL_COUNT} ImageNet labels, found {label_count}"
            )],
            vec![],
            vec![],
        ));
    }

    let (input_names, output_names, mut session) = match session_io_names(model, "BYO AutoTags") {
        Ok(v) => v,
        Err(e) => {
            return Ok(bad_report(
                capability,
                profile,
                vec![e.to_string()],
                vec![],
                vec![],
            ));
        }
    };
    if input_names.is_empty() || output_names.is_empty() {
        return Ok(bad_report(
            capability,
            profile,
            vec!["model has no inputs or outputs".into()],
            input_names,
            output_names,
        ));
    }

    let in_name = input_names[0].clone();
    let out_name = output_names[0].clone();
    let mut warnings = Vec::new();
    let mut worked_size = None;
    let mut reasons = Vec::new();

    for &size in AUTOTAGS_INPUT_SIZES {
        let n = (size as usize) * (size as usize) * 3;
        let zeros = vec![0f32; n];
        let tensor = match Tensor::from_array(([1usize, 3, size as usize, size as usize], zeros)) {
            Ok(t) => t,
            Err(e) => {
                reasons.push(format!("could not build {size}×{size} tensor: {e}"));
                continue;
            }
        };
        match session.run(ort::inputs![in_name.as_str() => tensor]) {
            Ok(outputs) => {
                let Some(value) = outputs.get(out_name.as_str()) else {
                    reasons.push(format!("missing output '{out_name}' at size {size}"));
                    continue;
                };
                match value.try_extract_tensor::<f32>() {
                    Ok((_shape, data)) => {
                        if data.len() < AUTOTAGS_LABEL_COUNT {
                            reasons.push(format!(
                                "output len {} < {AUTOTAGS_LABEL_COUNT} at size {size}",
                                data.len()
                            ));
                            continue;
                        }
                        worked_size = Some(size);
                        reasons.push(format!(
                            "accepted NCHW {size}×{size} → {AUTOTAGS_LABEL_COUNT}+ logits"
                        ));
                        break;
                    }
                    Err(e) => reasons.push(format!("output not f32 at size {size}: {e}")),
                }
            }
            Err(e) => reasons.push(format!("inference failed at size {size}: {e}")),
        }
    }

    if let Some(size) = worked_size {
        if in_name != "pixel_values" {
            warnings.push(format!(
                "input name is '{in_name}' (first input used; preferred 'pixel_values')"
            ));
        }
        Ok(ok_report(
            capability,
            profile,
            reasons,
            warnings,
            Some(size),
            None,
            input_names,
            output_names,
        ))
    } else {
        Ok(bad_report(
            capability,
            profile,
            if reasons.is_empty() {
                vec!["no compatible input size (tried 224 and 256)".into()]
            } else {
                reasons
            },
            input_names,
            output_names,
        ))
    }
}

/// Evaluate CLIP vision ONNX.
pub fn evaluate_clip_vision(model: &Path) -> AppResult<EvalReport> {
    let capability = Capability::SemanticSearch;
    let profile = ProfileKind::ClipVision;
    if !model.is_file() {
        return Ok(bad_report(
            capability,
            profile,
            vec!["vision model file not found".into()],
            vec![],
            vec![],
        ));
    }
    let (input_names, output_names, mut session) = match session_io_names(model, "BYO CLIP vision") {
        Ok(v) => v,
        Err(e) => {
            return Ok(bad_report(capability, profile, vec![e.to_string()], vec![], vec![]));
        }
    };
    let mut reasons = Vec::new();
    if !input_names.iter().any(|n| n == CLIP_PIXEL_VALUES) {
        reasons.push(format!("missing required input '{CLIP_PIXEL_VALUES}'"));
    }
    if !output_names.iter().any(|n| n == CLIP_IMAGE_EMBEDS) {
        reasons.push(format!("missing required output '{CLIP_IMAGE_EMBEDS}'"));
    }
    if !reasons.is_empty() {
        return Ok(bad_report(
            capability,
            profile,
            reasons,
            input_names,
            output_names,
        ));
    }

    let size = CLIP_IMAGE_SIZE as usize;
    let zeros = vec![0f32; 3 * size * size];
    let tensor = Tensor::from_array(([1usize, 3, size, size], zeros))
        .map_err(|e| AppError::msg(format!("clip vision tensor: {e}")))?;
    let outputs = match session.run(ort::inputs![CLIP_PIXEL_VALUES => tensor]) {
        Ok(o) => o,
        Err(e) => {
            return Ok(bad_report(
                capability,
                profile,
                vec![format!("vision smoke inference failed: {e}")],
                input_names,
                output_names,
            ));
        }
    };
    let value = outputs.get(CLIP_IMAGE_EMBEDS).ok_or_else(|| {
        AppError::msg(format!("missing output {CLIP_IMAGE_EMBEDS}"))
    })?;
    let (_shape, data) = match value.try_extract_tensor::<f32>() {
        Ok(v) => v,
        Err(e) => {
            return Ok(bad_report(
                capability,
                profile,
                vec![format!("image_embeds not f32: {e}")],
                input_names,
                output_names,
            ));
        }
    };
    if data.len() < CLIP_EMBED_DIM {
        return Ok(bad_report(
            capability,
            profile,
            vec![format!(
                "embedding dim {} < required {CLIP_EMBED_DIM}",
                data.len()
            )],
            input_names,
            output_names,
        ));
    }
    Ok(ok_report(
        capability,
        profile,
        vec![format!(
            "CLIP vision OK · {CLIP_PIXEL_VALUES}→{CLIP_IMAGE_EMBEDS} · dim≥{CLIP_EMBED_DIM}"
        )],
        if data.len() > CLIP_EMBED_DIM {
            vec![format!(
                "output dim {} > {CLIP_EMBED_DIM}; first {CLIP_EMBED_DIM} will be used",
                data.len()
            )]
        } else {
            vec![]
        },
        Some(CLIP_IMAGE_SIZE),
        Some(CLIP_EMBED_DIM),
        input_names,
        output_names,
    ))
}

/// Evaluate CLIP text ONNX (+ tokenizer.json presence).
pub fn evaluate_clip_text(model: &Path, tokenizer: &Path) -> AppResult<EvalReport> {
    let capability = Capability::SemanticSearch;
    let profile = ProfileKind::ClipText;
    if !model.is_file() {
        return Ok(bad_report(
            capability,
            profile,
            vec!["text model file not found".into()],
            vec![],
            vec![],
        ));
    }
    if !tokenizer.is_file() {
        return Ok(bad_report(
            capability,
            profile,
            vec!["tokenizer.json not found".into()],
            vec![],
            vec![],
        ));
    }
    // Ensure tokenizer JSON parses.
    if let Err(e) = std::fs::read_to_string(tokenizer)
        .map_err(|e| e.to_string())
        .and_then(|s| {
            serde_json::from_str::<serde_json::Value>(&s).map_err(|e| e.to_string())
        })
    {
        return Ok(bad_report(
            capability,
            profile,
            vec![format!("tokenizer.json invalid: {e}")],
            vec![],
            vec![],
        ));
    }

    let (input_names, output_names, mut session) = match session_io_names(model, "BYO CLIP text") {
        Ok(v) => v,
        Err(e) => {
            return Ok(bad_report(capability, profile, vec![e.to_string()], vec![], vec![]));
        }
    };
    let mut reasons = Vec::new();
    for required in [CLIP_INPUT_IDS, CLIP_ATTENTION_MASK] {
        if !input_names.iter().any(|n| n == required) {
            reasons.push(format!("missing required input '{required}'"));
        }
    }
    if !output_names.iter().any(|n| n == CLIP_TEXT_EMBEDS) {
        reasons.push(format!("missing required output '{CLIP_TEXT_EMBEDS}'"));
    }
    if !reasons.is_empty() {
        return Ok(bad_report(
            capability,
            profile,
            reasons,
            input_names,
            output_names,
        ));
    }

    let ids = vec![0i64; CLIP_SEQ_LEN];
    let mask = vec![1i64; CLIP_SEQ_LEN];
    let ids_t = Tensor::from_array(([1usize, CLIP_SEQ_LEN], ids))
        .map_err(|e| AppError::msg(format!("clip text ids: {e}")))?;
    let mask_t = Tensor::from_array(([1usize, CLIP_SEQ_LEN], mask))
        .map_err(|e| AppError::msg(format!("clip text mask: {e}")))?;
    let outputs = match session.run(ort::inputs![
        CLIP_INPUT_IDS => ids_t,
        CLIP_ATTENTION_MASK => mask_t
    ]) {
        Ok(o) => o,
        Err(e) => {
            return Ok(bad_report(
                capability,
                profile,
                vec![format!("text smoke inference failed: {e}")],
                input_names,
                output_names,
            ));
        }
    };
    let value = match outputs.get(CLIP_TEXT_EMBEDS) {
        Some(v) => v,
        None => {
            return Ok(bad_report(
                capability,
                profile,
                vec![format!("missing output {CLIP_TEXT_EMBEDS}")],
                input_names,
                output_names,
            ));
        }
    };
    let (_shape, data) = match value.try_extract_tensor::<f32>() {
        Ok(v) => v,
        Err(e) => {
            return Ok(bad_report(
                capability,
                profile,
                vec![format!("text_embeds not f32: {e}")],
                input_names,
                output_names,
            ));
        }
    };
    if data.len() < CLIP_EMBED_DIM {
        return Ok(bad_report(
            capability,
            profile,
            vec![format!(
                "embedding dim {} < required {CLIP_EMBED_DIM}",
                data.len()
            )],
            input_names,
            output_names,
        ));
    }
    let _ = profiles::ProfileKind::ClipText;
    Ok(ok_report(
        capability,
        profile,
        vec![format!(
            "CLIP text OK · ids/mask→{CLIP_TEXT_EMBEDS} · dim≥{CLIP_EMBED_DIM}"
        )],
        vec![],
        None,
        Some(CLIP_EMBED_DIM),
        input_names,
        output_names,
    ))
}

/// Evaluate a full CLIP bundle (vision + text + tokenizer).
pub fn evaluate_clip_bundle(
    vision: &Path,
    text: &Path,
    tokenizer: &Path,
) -> AppResult<EvalReport> {
    let vision_report = evaluate_clip_vision(vision)?;
    if !vision_report.compatible {
        return Ok(vision_report);
    }
    let text_report = evaluate_clip_text(text, tokenizer)?;
    if !text_report.compatible {
        return Ok(text_report);
    }
    let mut reasons = vision_report.reasons;
    reasons.extend(text_report.reasons);
    let mut warnings = vision_report.warnings;
    warnings.extend(text_report.warnings);
    let mut input_names = vision_report.input_names;
    input_names.extend(text_report.input_names);
    let mut output_names = vision_report.output_names;
    output_names.extend(text_report.output_names);
    Ok(ok_report(
        Capability::SemanticSearch,
        ProfileKind::ClipVision,
        reasons,
        warnings,
        Some(CLIP_IMAGE_SIZE),
        Some(CLIP_EMBED_DIM),
        input_names,
        output_names,
    ))
}
