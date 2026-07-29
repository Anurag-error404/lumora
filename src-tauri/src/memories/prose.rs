//! On-device memory prose via LaMini-Flan-T5-248M (quantized ONNX).
//!
//! User-initiated install + SHA-256 pin (same pattern as captions). Inference is
//! on-demand when opening/enriching a memory — never a cloud call.

use std::path::PathBuf;
use std::sync::OnceLock;

use ort::session::Session;
use ort::value::Tensor;
use parking_lot::Mutex;
use rusqlite::{params, Connection, OptionalExtension};
use sha2::{Digest, Sha256};
use tokenizers::Tokenizer;

use crate::error::{AppError, AppResult};
use crate::ml::{self, catalog::ModelKind};
use crate::ml::session;

const DECODER_START_TOKEN_ID: i64 = 0;
const EOS_TOKEN_ID: i64 = 1;
const MAX_NEW_TOKENS: usize = 48;
const MODEL_ID: &str = "lamini-flan-t5-248m";

#[derive(Debug, Clone)]
pub struct ProseModelPaths {
    pub encoder: PathBuf,
    pub decoder: PathBuf,
    pub tokenizer: PathBuf,
}

pub fn active_bundle(app_data: &std::path::Path) -> String {
    let preferred = crate::preferences::load(app_data)
        .map(|p| p.ai.prose_model)
        .unwrap_or_else(|_| MODEL_ID.into());
    ml::library::resolve_active(ml::library::Capability::MemoryProse, &preferred)
        .bundle
        .unwrap_or(ml::catalog::PROSE_BUNDLE)
        .to_string()
}

pub fn prose_ready(conn: &Connection) -> AppResult<bool> {
    prose_ready_bundle(conn, ml::catalog::PROSE_BUNDLE)
}

pub fn prose_ready_bundle(conn: &Connection, bundle: &str) -> AppResult<bool> {
    ml::catalog::bundle(bundle)
        .try_fold(true, |_, entry| Ok(ml::installed_row(conn, entry.id)?.is_some()))
}

pub fn model_paths_for(conn: &Connection, bundle: &str) -> AppResult<ProseModelPaths> {
    let mut encoder = None;
    let mut decoder = None;
    let mut tokenizer = None;
    for entry in ml::catalog::bundle(bundle) {
        let path = ml::require_path(conn, entry.id)?;
        match entry.kind {
            ModelKind::ProseEncoder => encoder = Some(path),
            ModelKind::ProseDecoder => decoder = Some(path),
            ModelKind::ProseTokenizer => tokenizer = Some(path),
            _ => {}
        }
    }
    let missing = |name| AppError::msg(format!("memory prose {name} missing in bundle {bundle}"));
    Ok(ProseModelPaths {
        encoder: encoder.ok_or_else(|| missing("encoder"))?,
        decoder: decoder.ok_or_else(|| missing("decoder"))?,
        tokenizer: tokenizer.ok_or_else(|| missing("tokenizer"))?,
    })
}

pub struct ProseEngine {
    encoder: Mutex<Session>,
    decoder: Mutex<Session>,
    tokenizer: Tokenizer,
}

impl ProseEngine {
    pub fn load(paths: &ProseModelPaths) -> AppResult<Self> {
        Ok(Self {
            encoder: Mutex::new(session::load_session(&paths.encoder, "memory prose encoder")?),
            decoder: Mutex::new(session::load_session(&paths.decoder, "memory prose decoder")?),
            tokenizer: Tokenizer::from_file(&paths.tokenizer)
                .map_err(|e| AppError::msg(format!("load memory prose tokenizer: {e}")))?,
        })
    }

    pub fn rewrite(&self, prompt: &str) -> AppResult<String> {
        let encoded = self
            .tokenizer
            .encode(prompt, true)
            .map_err(|e| AppError::msg(format!("tokenize memory prose prompt: {e}")))?;
        let ids: Vec<i64> = encoded.get_ids().iter().map(|&id| i64::from(id)).collect();
        let mask: Vec<i64> = encoded
            .get_attention_mask()
            .iter()
            .map(|&m| i64::from(m))
            .collect();
        if ids.is_empty() {
            return Err(AppError::msg("memory prose tokenizer returned empty ids"));
        }
        let seq_len = ids.len();
        let hidden = self.run_encoder(&ids, &mask, seq_len)?;
        let hidden_dim = hidden.len() / seq_len;
        if hidden_dim == 0 {
            return Err(AppError::msg("memory prose encoder returned empty hidden state"));
        }

        let mut decoder_ids = vec![DECODER_START_TOKEN_ID];
        let mut generated: Vec<u32> = Vec::new();
        for _ in 0..MAX_NEW_TOKENS {
            let next = self.run_decoder(&hidden, seq_len, hidden_dim, &decoder_ids, &mask)?;
            if next == EOS_TOKEN_ID {
                break;
            }
            generated.push(next as u32);
            decoder_ids.push(next);
        }

        let text = self
            .tokenizer
            .decode(&generated, true)
            .map_err(|e| AppError::msg(format!("decode memory prose: {e}")))?;
        Ok(text.trim().to_string())
    }

    fn run_encoder(&self, ids: &[i64], mask: &[i64], seq_len: usize) -> AppResult<Vec<f32>> {
        let mut enc = self.encoder.lock();
        let input_ids = Tensor::from_array(([1usize, seq_len], ids.to_vec()))
            .map_err(|e| AppError::msg(format!("prose encoder input_ids: {e}")))?;
        let attention = Tensor::from_array(([1usize, seq_len], mask.to_vec()))
            .map_err(|e| AppError::msg(format!("prose encoder attention_mask: {e}")))?;
        let outputs = enc
            .run(ort::inputs![
                "input_ids" => input_ids,
                "attention_mask" => attention,
            ])
            .map_err(|e| AppError::msg(format!("prose encoder run: {e}")))?;
        let (_shape, data) = outputs["last_hidden_state"]
            .try_extract_tensor::<f32>()
            .map_err(|e| AppError::msg(format!("prose encoder output: {e}")))?;
        Ok(data.to_vec())
    }

    fn run_decoder(
        &self,
        encoder_hidden: &[f32],
        enc_len: usize,
        hidden_dim: usize,
        decoder_ids: &[i64],
        enc_mask: &[i64],
    ) -> AppResult<i64> {
        // Xenova T5 decoder_model_quantized accepts:
        //   input_ids, encoder_hidden_states, encoder_attention_mask
        // (no decoder-side attention_mask).
        let mut dec = self.decoder.lock();
        let input_ids = Tensor::from_array(([1usize, decoder_ids.len()], decoder_ids.to_vec()))
            .map_err(|e| AppError::msg(format!("prose decoder input_ids: {e}")))?;
        let enc_attention = Tensor::from_array(([1usize, enc_len], enc_mask.to_vec()))
            .map_err(|e| AppError::msg(format!("prose decoder encoder_attention_mask: {e}")))?;
        let enc_states =
            Tensor::from_array(([1usize, enc_len, hidden_dim], encoder_hidden.to_vec()))
                .map_err(|e| AppError::msg(format!("prose decoder encoder_hidden_states: {e}")))?;
        let outputs = dec
            .run(ort::inputs![
                "input_ids" => input_ids,
                "encoder_attention_mask" => enc_attention,
                "encoder_hidden_states" => enc_states,
            ])
            .map_err(|e| AppError::msg(format!("prose decoder run: {e}")))?;
        let (shape, data) = outputs["logits"]
            .try_extract_tensor::<f32>()
            .map_err(|e| AppError::msg(format!("prose decoder logits: {e}")))?;
        // logits: [1, dec_len, vocab]
        let vocab = *shape.last().unwrap_or(&0) as usize;
        if vocab == 0 || data.len() < vocab {
            return Err(AppError::msg("prose decoder logits shape unexpected"));
        }
        let start = data.len() - vocab;
        let mut best_i = 0usize;
        let mut best_v = f32::NEG_INFINITY;
        for (i, &v) in data[start..].iter().enumerate() {
            if v > best_v {
                best_v = v;
                best_i = i;
            }
        }
        Ok(best_i as i64)
    }
}

static ENGINE: OnceLock<Mutex<Option<ProseEngine>>> = OnceLock::new();

fn engine_slot() -> &'static Mutex<Option<ProseEngine>> {
    ENGINE.get_or_init(|| Mutex::new(None))
}

pub fn invalidate_engine() {
    *engine_slot().lock() = None;
}

fn ensure_engine(conn: &Connection, app_data: &std::path::Path) -> AppResult<()> {
    let mut slot = engine_slot().lock();
    if slot.is_some() {
        return Ok(());
    }
    let bundle = active_bundle(app_data);
    let paths = model_paths_for(conn, &bundle)?;
    *slot = Some(ProseEngine::load(&paths)?);
    Ok(())
}

pub fn build_prompt(title: &str, subtitle: &str, quote: Option<&str>) -> String {
    let caption = quote
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("(none)");
    // Flan-style phrasing; "Memory:" / "photo memory" cues trigger refusals on LaMini.
    format!(
        "Summarize this photo memory in one warm sentence.\n\
         Title: {title}\n\
         Details: {subtitle}\n\
         Caption: {caption}\n\
         Sentence:"
    )
}

pub fn input_hash(prompt: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(prompt.as_bytes());
    hasher.update(MODEL_ID.as_bytes());
    hex::encode(hasher.finalize())
}

pub fn sanitize_prose(raw: &str, title: &str, subtitle: &str) -> Option<String> {
    let mut t = raw.trim().trim_matches('"').trim().to_string();
    if t.len() < 12 || t.len() > 280 {
        return None;
    }
    let lower = t.to_ascii_lowercase();
    for bad in [
        "i'm sorry",
        "i cannot",
        "i can't",
        "as an ai",
        "ethical",
        "against",
        "<pad>",
        "<unk>",
    ] {
        if lower.contains(bad) {
            return None;
        }
    }
    // Reject if it just repeats the title alone.
    if lower == title.to_ascii_lowercase() || lower == subtitle.to_ascii_lowercase() {
        return None;
    }
    // Strip leading labels if the model echoed the prompt cue.
    for prefix in ["Sentence:", "Memory:", "Summary:"] {
        if let Some(rest) = t.strip_prefix(prefix) {
            t = rest.trim().to_string();
            break;
        }
    }
    if t.len() < 12 {
        return None;
    }
    Some(t)
}

pub fn cached_prose(conn: &Connection, memory_id: &str, hash: &str) -> AppResult<Option<String>> {
    Ok(conn
        .query_row(
            "SELECT prose FROM memory_prose WHERE memory_id = ?1 AND input_hash = ?2",
            params![memory_id, hash],
            |r| r.get(0),
        )
        .optional()?)
}

pub fn store_prose(
    conn: &Connection,
    memory_id: &str,
    hash: &str,
    prose: &str,
) -> AppResult<()> {
    conn.execute(
        "INSERT INTO memory_prose (memory_id, input_hash, prose, model_id, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(memory_id) DO UPDATE SET
           input_hash = excluded.input_hash,
           prose = excluded.prose,
           model_id = excluded.model_id,
           created_at = excluded.created_at",
        params![
            memory_id,
            hash,
            prose,
            MODEL_ID,
            chrono::Utc::now().to_rfc3339()
        ],
    )?;
    Ok(())
}

pub fn clear_all(conn: &Connection) -> AppResult<usize> {
    let n = conn.execute("DELETE FROM memory_prose", [])?;
    Ok(n)
}

struct ProseJob {
    prompt: String,
    hash: String,
    cached: Option<String>,
}

fn prepare_prose(
    conn: &Connection,
    memory_id: &str,
    title: &str,
    subtitle: &str,
    quote: Option<&str>,
    enabled: bool,
) -> AppResult<Option<ProseJob>> {
    if !enabled {
        return Ok(None);
    }
    if !prose_ready(conn)? {
        return Ok(None);
    }
    let prompt = build_prompt(title, subtitle, quote);
    let hash = input_hash(&prompt);
    let cached = cached_prose(conn, memory_id, &hash)?;
    Ok(Some(ProseJob {
        prompt,
        hash,
        cached,
    }))
}

/// Run prose enrichment without holding the shared app DB across ONNX.
/// Returns the prose string when available (cached or freshly generated).
pub fn enrich_prose_unlocked(
    db_path: &std::path::Path,
    app_data: &std::path::Path,
    memory_id: &str,
    title: &str,
    subtitle: &str,
    quote: Option<&str>,
    enabled: bool,
) -> AppResult<Option<String>> {
    if !enabled {
        return Ok(None);
    }
    let job = {
        let conn = crate::state::open_db(db_path)?;
        let Some(job) = prepare_prose(&conn, memory_id, title, subtitle, quote, enabled)? else {
            return Ok(None);
        };
        if let Some(cached) = job.cached {
            return Ok(Some(cached));
        }
        // Load engine while we still have a short-lived connection for paths.
        ensure_engine(&conn, app_data)?;
        job
    };
    let raw = {
        let slot = engine_slot().lock();
        let Some(engine) = slot.as_ref() else {
            return Ok(None);
        };
        engine.rewrite(&job.prompt)?
    };
    let Some(clean) = sanitize_prose(&raw, title, subtitle) else {
        return Ok(None);
    };
    let conn = crate::state::open_db(db_path)?;
    store_prose(&conn, memory_id, &job.hash, &clean)?;
    Ok(Some(clean))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prompt_includes_slots() {
        let p = build_prompt("On this day", "12 photos", Some("a quiet street"));
        assert!(p.contains("On this day"));
        assert!(p.contains("12 photos"));
        assert!(p.contains("a quiet street"));
        assert!(p.contains("Sentence:"));
        assert!(!p.contains("Please write a short nostalgic"));
    }

    #[test]
    fn sanitize_rejects_refusals_and_short() {
        assert!(sanitize_prose("hi", "t", "s").is_none());
        assert!(sanitize_prose(
            "I'm sorry, I cannot write that.",
            "On this day",
            "12 photos"
        )
        .is_none());
        assert_eq!(
            sanitize_prose(
                "\"Looking back on this day with twelve quiet street photos.\"",
                "On this day",
                "12 photos"
            )
            .as_deref(),
            Some("Looking back on this day with twelve quiet street photos.")
        );
    }

    #[test]
    fn input_hash_is_stable() {
        let a = input_hash("hello");
        let b = input_hash("hello");
        let c = input_hash("hello!");
        assert_eq!(a, b);
        assert_ne!(a, c);
        assert_eq!(a.len(), 64);
    }

    /// Smoke the installed app models when present (skipped otherwise).
    #[test]
    fn smoke_decoder_accepts_t5_inputs() {
        let home = std::env::var_os("HOME").expect("HOME");
        let dir = std::path::PathBuf::from(home)
            .join("Library/Application Support/com.photovault.ai/models");
        let paths = ProseModelPaths {
            encoder: dir.join("prose-encoder.onnx"),
            decoder: dir.join("prose-decoder.onnx"),
            tokenizer: dir.join("prose-tokenizer.json"),
        };
        if !paths.encoder.exists() || !paths.decoder.exists() || !paths.tokenizer.exists() {
            eprintln!("skip smoke: prose models not installed at {}", dir.display());
            return;
        }
        let engine = ProseEngine::load(&paths).expect("load prose engine");
        let out = engine
            .rewrite(&build_prompt(
                "On this day",
                "3 photos from 2020",
                None,
            ))
            .expect("rewrite");
        eprintln!("prose smoke => {out:?}");
        assert!(!out.is_empty());
        assert!(
            sanitize_prose(&out, "On this day", "3 photos from 2020").is_some(),
            "expected usable prose, got {out:?}"
        );
    }
}
