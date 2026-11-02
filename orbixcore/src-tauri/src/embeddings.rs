//! Embedding-backed semantic retrieval (W3).
//!
//! memory.rs and knowledge.rs rank by substring / term-frequency, which
//! misses paraphrases ("chase the unpaid bill" vs "invoice reminder"). This
//! module adds real semantic ranking: embed the query and candidates via a
//! llama-server embedding endpoint (/v1/embeddings, mean pooling) and rank by
//! cosine similarity. It is a primitive: callers pass the endpoint and fall
//! back to keyword search when no embedding server is available, because the
//! generation models the app ships with do not produce good embeddings - a
//! dedicated embedding model (qwen3-embedding-0.6b, in the catalog) must
//! be loaded on an embeddings endpoint for this to beat keywords.

use serde::Deserialize;

#[derive(Deserialize)]
struct EmbeddingResponse {
    data: Vec<EmbeddingItem>,
}
#[derive(Deserialize)]
struct EmbeddingItem {
    embedding: Vec<f32>,
}

/// Embed one or more texts via an OpenAI-compatible /v1/embeddings endpoint.
/// The endpoint's server must run with --embeddings --pooling mean.
pub async fn embed(endpoint: &str, texts: &[String]) -> Result<Vec<Vec<f32>>, String> {
    if texts.is_empty() {
        return Ok(Vec::new());
    }
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| e.to_string())?;
    let resp = client
        .post(format!("{}/v1/embeddings", endpoint.trim_end_matches('/')))
        .json(&serde_json::json!({ "input": texts }))
        .send()
        .await
        .map_err(|e| format!("embeddings request failed: {}", e))?;
    if !resp.status().is_success() {
        return Err(format!("embeddings HTTP {}", resp.status().as_u16()));
    }
    let parsed: EmbeddingResponse = resp
        .json()
        .await
        .map_err(|e| format!("parse embeddings: {}", e))?;
    Ok(parsed.data.into_iter().map(|d| d.embedding).collect())
}

/// Cosine similarity of two vectors. Returns 0.0 for mismatched/empty.
pub fn cosine(a: &[f32], b: &[f32]) -> f32 {
    if a.is_empty() || a.len() != b.len() {
        return 0.0;
    }
    let mut dot = 0.0f32;
    let mut na = 0.0f32;
    let mut nb = 0.0f32;
    for i in 0..a.len() {
        dot += a[i] * b[i];
        na += a[i] * a[i];
        nb += b[i] * b[i];
    }
    if na == 0.0 || nb == 0.0 {
        return 0.0;
    }
    dot / (na.sqrt() * nb.sqrt())
}

/// Rank `candidates` against `query` by cosine similarity, returning
/// (index, score) sorted best-first. One embeddings call for the query plus
/// all candidates. `top_k` caps the result.
pub async fn semantic_rank(
    endpoint: &str,
    query: &str,
    candidates: &[String],
    top_k: usize,
) -> Result<Vec<(usize, f32)>, String> {
    if candidates.is_empty() {
        return Ok(Vec::new());
    }
    let mut inputs = Vec::with_capacity(candidates.len() + 1);
    inputs.push(query.to_string());
    inputs.extend_from_slice(candidates);
    let vecs = embed(endpoint, &inputs).await?;
    if vecs.len() != inputs.len() {
        return Err("embedding count mismatch".to_string());
    }
    let q = &vecs[0];
    let mut scored: Vec<(usize, f32)> = candidates
        .iter()
        .enumerate()
        .map(|(i, _)| (i, cosine(q, &vecs[i + 1])))
        .collect();
    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    scored.truncate(top_k);
    Ok(scored)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cosine_identical_is_one_orthogonal_is_zero() {
        let a = vec![1.0, 0.0, 0.0];
        assert!((cosine(&a, &a) - 1.0).abs() < 1e-6);
        assert!(cosine(&[1.0, 0.0], &[0.0, 1.0]).abs() < 1e-6);
        assert_eq!(cosine(&[], &[]), 0.0);
        assert_eq!(cosine(&[1.0], &[1.0, 2.0]), 0.0); // mismatched dims
    }

    #[test]
    fn rank_orders_by_similarity() {
        // Pure ranking check using hand cosine (no network).
        let q = vec![1.0f32, 0.0];
        let cands = vec![vec![0.9f32, 0.1], vec![0.0f32, 1.0], vec![1.0f32, 0.0]];
        let mut scored: Vec<(usize, f32)> = cands.iter().enumerate()
            .map(|(i, c)| (i, cosine(&q, c))).collect();
        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
        assert_eq!(scored[0].0, 2); // exact match first
        assert_eq!(scored[2].0, 1); // orthogonal last
    }

    #[tokio::test]
    #[ignore]
    async fn live_embed_and_rank() {
        // Needs an embeddings server on 8850 (--embeddings --pooling mean).
        // cargo test --bin orbixcore live_embed_and_rank -- --ignored --nocapture
        let ep = "http://127.0.0.1:8850";
        let cands = vec![
            "send an email about the overdue bill".to_string(),
            "a photo of a sunset over the ocean".to_string(),
            "schedule a dentist appointment".to_string(),
        ];
        let ranked = semantic_rank(ep, "invoice reminder", &cands, 3).await.expect("rank");
        println!("LIVE RANK: {:?}", ranked);
        assert_eq!(ranked[0].0, 0, "overdue-bill should rank first for 'invoice reminder'");
    }
}
