use anyhow::Result;
use axum::{extract::{Path, Query, State}, http::StatusCode, routing::{get, post}, Json, Router};
use core::persist::{load_index_header, load_postings_for_term, IndexPaths};
use core::tokenizer::tokenize;
use core::{DocId, DocMeta, TermId};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use tower_http::cors::{Any, CorsLayer, AllowOrigin};

#[derive(Deserialize)]
pub struct SearchParams {
    pub q: String,
    #[serde(default = "default_k")] 
    pub k: usize,
}
fn default_k() -> usize { 10 }

#[derive(Serialize)]
pub struct SearchResponse {
    pub query: String,
    pub took_ms: u128, // deprecated, kept for backward compatibility
    pub took_s: f64,
    pub total_hits: usize,
    pub results: Vec<SearchHit>,
}

#[derive(Serialize)]
pub struct SearchHit {
    pub doc_id: u32,
    pub score: f32,
    pub title: String,
    pub url: Option<String>,
    pub snippet: Option<String>,
}

#[derive(Clone)]
pub struct AppState {
    pub index_paths_root: PathBuf,
    pub dictionary: HashMap<String, TermId>,
    pub df: Vec<u32>,
    pub docs: HashMap<DocId, DocMeta>,
    pub num_docs: u32,
    pub admin_token: Option<String>,
}

pub fn build_app(index_dir: String) -> Result<Router> {
    // Load index header at startup
    let index_paths = IndexPaths::new(&index_dir);
    let (dictionary, df, docs, meta) = load_index_header(&index_paths)?;
    let admin_token = std::env::var("ADMIN_TOKEN").ok();
    let app_state = AppState { index_paths_root: PathBuf::from(&index_dir), dictionary, df, docs, num_docs: meta.num_docs, admin_token };

    // CORS: read CORS_ALLOW_ORIGIN (comma-separated) or allow Any by default
    let cors = match std::env::var("CORS_ALLOW_ORIGIN") {
        Ok(val) => {
            let origins: Vec<_> = val
                .split(',')
                .filter_map(|s| s.trim().parse().ok())
                .collect();
            if origins.is_empty() {
                CorsLayer::new().allow_origin(Any).allow_methods(Any).allow_headers(Any)
            } else {
                CorsLayer::new().allow_origin(AllowOrigin::list(origins)).allow_methods(Any).allow_headers(Any)
            }
        }
        Err(_) => CorsLayer::new().allow_origin(Any).allow_methods(Any).allow_headers(Any),
    };

    let app = Router::new()
        .route("/health", get(|| async { "ok" }))
        .route("/search", get(search_handler))
        .route("/doc/:doc_id", get(doc_handler))
        .route("/index/batch", post(index_batch))
        .route("/index/commit", post(index_commit))
        .with_state(app_state)
        .layer(cors);
    Ok(app)
}

pub async fn search_handler(State(state): State<AppState>, Query(params): Query<SearchParams>) -> Json<SearchResponse> {
    let start = std::time::Instant::now();
    // Tokenize query and build tf map
    let q_tokens = tokenize(&params.q);
    let mut tf_q_raw: HashMap<TermId, u32> = HashMap::new();
    for (term, _pos) in q_tokens {
        if let Some(&tid) = state.dictionary.get(&term) {
            *tf_q_raw.entry(tid).or_insert(0) += 1;
        }
    }
    // Edge case: empty after filtering
    if tf_q_raw.is_empty() {
        let elapsed = start.elapsed();
        return Json(SearchResponse { query: params.q, took_ms: elapsed.as_millis(), took_s: elapsed.as_secs_f64(), total_hits: 0, results: vec![] });
    }

    // Compute normalized query weights
    let n = state.num_docs.max(1);
    let mut q_weights: HashMap<TermId, f32> = HashMap::new();
    for (tid, tf_raw) in tf_q_raw.iter() {
        let tf = if *tf_raw > 0 { 1.0 + (*tf_raw as f32).ln() } else { 0.0 };
        let df_t = *state.df.get(*tid as usize).unwrap_or(&1).max(&1);
        let idf = ((n as f32) / (df_t as f32)).ln();
        q_weights.insert(*tid, tf * idf);
    }
    let mut norm = 0.0f32;
    for w in q_weights.values() { norm += w * w; }
    norm = norm.sqrt();
    if norm == 0.0 { norm = 1.0; }
    for w in q_weights.values_mut() { *w /= norm; }

    // Aggregate scores from postings
    let mut scores: HashMap<DocId, f32> = HashMap::new();
    let paths = IndexPaths::new(&state.index_paths_root);
    for (tid, q_w) in q_weights.iter() {
        if let Ok(postings) = load_postings_for_term(&paths, *tid) {
            for p in postings {
                let contrib = p.weight * *q_w; // cosine since doc weights are normalized
                *scores.entry(p.doc_id).or_insert(0.0) += contrib;
            }
        }
    }

    let mut scored: Vec<(DocId, f32)> = scores.into_iter().collect();
    let k = params.k.max(1).min(100);
    // partial sort for top-k
    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    let total_hits = scored.len();
    let topk = scored.into_iter().take(k);

    // Build results with snippets
    let mut results: Vec<SearchHit> = Vec::new();
    // Capture raw query terms for highlighting
    let raw_terms: Vec<String> = params
        .q
        .split_whitespace()
        .map(|s| s.to_string())
        .collect();
    for (doc_id, score) in topk {
        if let Some(meta) = state.docs.get(&doc_id) {
            let snippet = meta
                .text_path
                .as_ref()
                .and_then(|rel| snippet_from_file(&state.index_paths_root.join(rel), &raw_terms));
            results.push(SearchHit { doc_id, score, title: meta.title.clone(), url: meta.url.clone(), snippet });
        }
    }

    let elapsed = start.elapsed();
    Json(SearchResponse { query: params.q, took_ms: elapsed.as_millis(), took_s: elapsed.as_secs_f64(), total_hits, results })
}

pub async fn doc_handler(State(state): State<AppState>, Path(doc_id): Path<u32>) -> Json<serde_json::Value> {
    if let Some(meta) = state.docs.get(&doc_id) {
        let mut obj = serde_json::json!({
            "doc_id": doc_id,
            "title": meta.title,
            "url": meta.url,
        });
        if let Some(rel) = &meta.text_path {
            if let Ok(text) = std::fs::read_to_string(state.index_paths_root.join(rel)) {
                obj["text"] = serde_json::Value::String(text);
            }
        }
        return Json(obj);
    }
    Json(serde_json::json!({ "error": "not found" }))
}

fn snippet_from_file(path: &PathBuf, raw_terms: &Vec<String>) -> Option<String> {
    let text = std::fs::read_to_string(path).ok()?;
    if text.is_empty() { return None; }
    // find first match (case-insensitive) of any raw term
    let mut first_idx: Option<usize> = None;
    for term in raw_terms {
        if term.trim().is_empty() { continue; }
        if let Some(pos) = find_case_insensitive(&text, term) { first_idx = Some(pos); break; }
    }
    let snippet = match first_idx {
        Some(idx) => {
            let start = idx.saturating_sub(100);
            let end = (idx + 200).min(text.len());
            text[start..end].to_string()
        }
        None => text.chars().take(200).collect(),
    };
    Some(highlight_terms(&snippet, raw_terms))
}

fn find_case_insensitive(haystack: &str, needle: &str) -> Option<usize> {
    let h = haystack.to_lowercase();
    let n = needle.to_lowercase();
    h.find(&n)
}

fn highlight_terms(snippet: &str, terms: &Vec<String>) -> String {
    let mut s = snippet.to_string();
    for t in terms {
        if t.trim().is_empty() { continue; }
        let pat = regex::RegexBuilder::new(&regex::escape(t))
            .case_insensitive(true)
            .build()
            .unwrap();
        s = pat.replace_all(&s, |caps: &regex::Captures| format!("<em>{}</em>", &caps[0])).to_string();
    }
    s
}

// --- Admin endpoints (stubs) ---
async fn index_batch(State(state): State<AppState>, headers: axum::http::HeaderMap, Json(_docs): Json<serde_json::Value>) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    authorize(&state, &headers)?;
    Err((StatusCode::NOT_IMPLEMENTED, "Incremental indexing not implemented".into()))
}

async fn index_commit(State(state): State<AppState>, headers: axum::http::HeaderMap) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    authorize(&state, &headers)?;
    Err((StatusCode::NOT_IMPLEMENTED, "Commit not implemented".into()))
}

fn authorize(state: &AppState, headers: &axum::http::HeaderMap) -> Result<(), (StatusCode, String)> {
    let required = match &state.admin_token {
        Some(t) => t,
        None => return Err((StatusCode::UNAUTHORIZED, "ADMIN_TOKEN not set".into())),
    };
    let provided = headers.get("X-ADMIN-TOKEN").and_then(|v| v.to_str().ok()).unwrap_or("");
    if provided == required {
        Ok(())
    } else {
        Err((StatusCode::UNAUTHORIZED, "invalid admin token".into()))
    }
}
