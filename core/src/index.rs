use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub type TermId = u32;
pub type DocId = u32;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocMeta {
    pub external_id: String,
    pub title: String,
    pub url: Option<String>,
    /// Relative path to the stored full text for snippet extraction, e.g., texts/{doc_id}.txt
    pub text_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Posting {
    pub doc_id: DocId,
    pub weight: f32, // normalized tf-idf weight
}

#[derive(Default, Serialize, Deserialize)]
pub struct InvertedIndex {
    pub dictionary: HashMap<String, TermId>,
    pub df: Vec<u32>,
    pub postings: HashMap<TermId, Vec<Posting>>, // postings sorted by doc_id
    pub docs: HashMap<DocId, DocMeta>,
    pub num_docs: u32,
}

impl InvertedIndex {
    pub fn new() -> Self { Self::default() }
}
