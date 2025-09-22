use axum::http::{Request, StatusCode};
use axum::Router;
use core::persist::{save_dictionary, save_docs, save_meta, save_postings_for_term, IndexPaths, MetaFile};
use core::{DocId, DocMeta, Posting, TermId};
use http_body_util::BodyExt;
use hyper::body::Bytes;
use hyper::body::Incoming as IncomingBody;
use hyper::Request as HyperRequest;
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use tempfile::tempdir;

fn build_tiny_index(dir: &std::path::Path) {
    let paths = IndexPaths::new(dir);
    fs::create_dir_all(dir.join("postings")).unwrap();
    fs::create_dir_all(dir.join("texts")).unwrap();

    // Dictionary with one term: "rust" -> term_id 0
    let mut dict: HashMap<String, TermId> = HashMap::new();
    dict.insert("rust".to_string(), 0);
    let df = vec![2u32]; // appears in both docs
    save_dictionary(&paths, &(dict, df)).unwrap();

    // Docs metadata
    let mut docs: HashMap<DocId, DocMeta> = HashMap::new();
    docs.insert(0, DocMeta { external_id: "doc0".into(), title: "Doc 0".into(), url: None, text_path: Some("texts/0.txt".into()) });
    docs.insert(1, DocMeta { external_id: "doc1".into(), title: "Doc 1".into(), url: None, text_path: Some("texts/1.txt".into()) });
    save_docs(&paths, &docs).unwrap();

    // Texts
    fs::write(dir.join("texts/0.txt"), "Rust is great. rust systems programming.").unwrap();
    fs::write(dir.join("texts/1.txt"), "Learning rust.").unwrap();

    // Postings for term 0 with normalized weights precomputed.
    // Let doc0 have higher weight than doc1.
    let postings = vec![
        Posting { doc_id: 0, weight: 0.8 },
        Posting { doc_id: 1, weight: 0.6 },
    ];
    save_postings_for_term(&paths, 0, &postings).unwrap();

    // Meta
    let meta = MetaFile { num_docs: 2, created_at: "2024-01-01T00:00:00Z".into(), version: 1 };
    save_meta(&paths, &meta).unwrap();
}

async fn call(app: Router, uri: &str) -> (StatusCode, Bytes) {
    let req: HyperRequest<hyper::body::Body> = Request::get(uri).body(hyper::body::Body::empty()).unwrap().map(
        |b| match b {}
    );
    let svc = app.into_service();
    let resp = tower::ServiceExt::oneshot(svc, req).await.unwrap();
    let status = resp.status();
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    (status, body)
}

#[tokio::test]
async fn search_returns_ranked_results() {
    let dir = tempdir().unwrap();
    build_tiny_index(dir.path());
    let app = server::build_app(dir.path().to_string_lossy().to_string()).unwrap();

    let (status, body) = call(app, "/search?q=rust&k=2").await;
    assert_eq!(status, StatusCode::OK);
    let json: Value = serde_json::from_slice(&body).unwrap();
    let arr = json["results"].as_array().unwrap();
    assert_eq!(arr.len(), 2);
    let d0 = arr[0]["doc_id"].as_u64().unwrap();
    let d1 = arr[1]["doc_id"].as_u64().unwrap();
    assert_eq!(d0, 0);
    assert_eq!(d1, 1);
}
