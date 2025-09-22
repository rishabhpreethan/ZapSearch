use anyhow::Result;
use clap::{Parser, Subcommand};
use core::persist::{save_dictionary, save_doc_id_map, save_docs, save_meta, save_postings_for_term, IndexPaths, MetaFile};
use core::tokenizer::tokenize;
use core::{DocId, DocMeta, Posting, TermId};
use serde::Deserialize;
use tracing_subscriber::{EnvFilter, fmt};
use walkdir::WalkDir;

use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

#[derive(Debug, Deserialize)]
struct InputDoc {
    id: String,
    title: String,
    body: String,
    url: Option<String>,
    timestamp: Option<String>,
    #[serde(default)]
    meta: Option<serde_json::Value>,
}

#[derive(Parser)]
#[command(name = "indexer")] 
#[command(about = "Build and manage TF-IDF inverted index", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Build the index from input JSON/JSONL files or a directory
    Build {
        /// Input path (file or directory)
        #[arg(long)]
        input: String,
        /// Output index directory
        #[arg(long)]
        output: String,
        /// Use smoothed IDF = ln(1 + N/df) instead of ln(N/df)
        #[arg(long, default_value_t = false)]
        smoothed_idf: bool,
    },
}

fn main() -> Result<()> {
    fmt().with_env_filter(EnvFilter::from_default_env()).init();
    let cli = Cli::parse();

    match cli.command {
        Commands::Build { input, output, smoothed_idf } => {
            build_index(&input, &output, smoothed_idf)
        }
    }
}

fn build_index(input: &str, output: &str, smoothed_idf: bool) -> Result<()> {
    let input_path = Path::new(input);
    let out_paths = IndexPaths::new(output);
    fs::create_dir_all(&out_paths.root)?;
    fs::create_dir_all(out_paths.root.join("texts"))?;

    // Accumulators
    let mut next_doc_id: DocId = 0;
    let mut next_term_id: TermId = 0;
    let mut dictionary: HashMap<String, TermId> = HashMap::new();
    let mut df: Vec<u32> = Vec::new();
    let mut postings_raw: HashMap<TermId, Vec<(DocId, u32)>> = HashMap::new();
    let mut docs: HashMap<DocId, DocMeta> = HashMap::new();
    let mut doc_id_map: HashMap<String, DocId> = HashMap::new();

    let mut files: Vec<PathBuf> = Vec::new();
    if input_path.is_dir() {
        for entry in WalkDir::new(input_path).into_iter().filter_map(|e| e.ok()) {
            let p = entry.path();
            if p.is_file() {
                if let Some(ext) = p.extension().and_then(|s| s.to_str()) {
                    if matches!(ext, "json" | "jsonl") {
                        files.push(p.to_path_buf());
                    }
                }
            }
        }
    } else if input_path.is_file() {
        files.push(input_path.to_path_buf());
    }

    for file in files {
        if file.extension().and_then(|s| s.to_str()) == Some("jsonl") {
            index_jsonl(&file, &mut next_doc_id, &mut next_term_id, &mut dictionary, &mut df, &mut postings_raw, &mut docs, &mut doc_id_map, &out_paths)?;
        } else {
            index_json(&file, &mut next_doc_id, &mut next_term_id, &mut dictionary, &mut df, &mut postings_raw, &mut docs, &mut doc_id_map, &out_paths)?;
        }
    }

    let num_docs = next_doc_id as u32;
    tracing::info!(num_docs, num_terms = dictionary.len(), "ingested documents");

    // Compute TF-IDF and normalize
    let n = num_docs.max(1);
    // Ensure df length matches highest term id + 1
    df.resize(next_term_id as usize, 0);

    let mut doc_norms: Vec<f32> = vec![0.0; num_docs as usize];
    // First pass: compute tfidf and accumulate norms
    for (term_id, plist) in postings_raw.iter_mut() {
        let df_t = df[*term_id as usize].max(1);
        let idf = if smoothed_idf { (1.0 + (n as f32) / (df_t as f32)).ln() } else { ((n as f32) / (df_t as f32)).ln() };
        for (doc_id, tf_raw) in plist.iter_mut() {
            let tf = if *tf_raw > 0 { 1.0 + (*tf_raw as f32).ln() } else { 0.0 };
            let tfidf = tf * idf;
            doc_norms[*doc_id as usize] += tfidf * tfidf;
            // temporarily store tfidf back in tf_raw slot by casting via bits (will convert in second pass)
            *tf_raw = f32_to_u32(tfidf);
        }
    }
    for dn in doc_norms.iter_mut() {
        *dn = dn.sqrt();
        if *dn == 0.0 { *dn = 1.0; }
    }

    // Second pass: create normalized postings and persist per term
    for (term_id, plist) in postings_raw.into_iter() {
        let mut out_postings: Vec<Posting> = Vec::with_capacity(plist.len());
        for (doc_id, tfidf_bits) in plist.into_iter() {
            let tfidf = u32_to_f32(tfidf_bits);
            let norm = doc_norms[doc_id as usize];
            let weight = tfidf / norm;
            out_postings.push(Posting { doc_id, weight });
        }
        // Sort by doc_id per spec
        out_postings.sort_by_key(|p| p.doc_id);
        save_postings_for_term(&out_paths, term_id, &out_postings)?;
    }

    // Persist dictionary, docs, doc_id_map, meta
    save_dictionary(&out_paths, &(dictionary.clone(), df.clone()))?;
    save_docs(&out_paths, &docs)?;
    save_doc_id_map(&out_paths, &doc_id_map)?;
    let meta = MetaFile {
        num_docs: n,
        created_at: time::OffsetDateTime::now_utc().format(&time::format_description::well_known::Rfc3339).unwrap_or_else(|_| "".into()),
        version: 1,
    };
    save_meta(&out_paths, &meta)?;

    tracing::info!(output, "index build complete");
    Ok(())
}

fn index_jsonl(file: &Path, next_doc_id: &mut DocId, next_term_id: &mut TermId, dictionary: &mut HashMap<String, TermId>, df: &mut Vec<u32>, postings_raw: &mut HashMap<TermId, Vec<(DocId, u32)>>, docs: &mut HashMap<DocId, DocMeta>, doc_id_map: &mut HashMap<String, DocId>, out_paths: &IndexPaths) -> Result<()> {
    let f = File::open(file)?;
    let reader = BufReader::new(f);
    for line in reader.lines() {
        let line = line?;
        if line.trim().is_empty() { continue; }
        let doc: InputDoc = serde_json::from_str(&line)?;
        ingest_doc(doc, next_doc_id, next_term_id, dictionary, df, postings_raw, docs, doc_id_map, out_paths)?;
    }
    Ok(())
}

fn index_json(file: &Path, next_doc_id: &mut DocId, next_term_id: &mut TermId, dictionary: &mut HashMap<String, TermId>, df: &mut Vec<u32>, postings_raw: &mut HashMap<TermId, Vec<(DocId, u32)>>, docs: &mut HashMap<DocId, DocMeta>, doc_id_map: &mut HashMap<String, DocId>, out_paths: &IndexPaths) -> Result<()> {
    let f = File::open(file)?;
    let reader = BufReader::new(f);
    let json: serde_json::Value = serde_json::from_reader(reader)?;
    match json {
        serde_json::Value::Array(arr) => {
            for v in arr {
                let doc: InputDoc = serde_json::from_value(v)?;
                ingest_doc(doc, next_doc_id, next_term_id, dictionary, df, postings_raw, docs, doc_id_map, out_paths)?;
            }
        }
        serde_json::Value::Object(_) => {
            let doc: InputDoc = serde_json::from_value(json)?;
            ingest_doc(doc, next_doc_id, next_term_id, dictionary, df, postings_raw, docs, doc_id_map, out_paths)?;
        }
        _ => {}
    }
    Ok(())
}

fn ingest_doc(doc: InputDoc, next_doc_id: &mut DocId, next_term_id: &mut TermId, dictionary: &mut HashMap<String, TermId>, df: &mut Vec<u32>, postings_raw: &mut HashMap<TermId, Vec<(DocId, u32)>>, docs: &mut HashMap<DocId, DocMeta>, doc_id_map: &mut HashMap<String, DocId>, out_paths: &IndexPaths) -> Result<()> {
    let doc_id = *next_doc_id;
    *next_doc_id += 1;
    doc_id_map.insert(doc.id.clone(), doc_id);

    // Tokenize body and compute term frequencies
    let tokens = tokenize(&doc.body);
    let mut tf_counts: HashMap<TermId, u32> = HashMap::new();
    let mut seen_in_doc: HashSet<TermId> = HashSet::new();
    for (term, _pos) in tokens {
        let tid = *dictionary.entry(term).or_insert_with(|| {
            let id = *next_term_id;
            *next_term_id += 1;
            // ensure df vec capacity
            if df.len() <= id as usize { df.resize(id as usize + 1, 0); }
            id
        });
        *tf_counts.entry(tid).or_insert(0) += 1;
        if !seen_in_doc.contains(&tid) {
            df[tid as usize] += 1;
            seen_in_doc.insert(tid);
        }
    }

    for (tid, tf_raw) in tf_counts.into_iter() {
        postings_raw.entry(tid).or_default().push((doc_id, tf_raw));
    }

    // Write text for snippet extraction
    let text_rel = format!("texts/{}.txt", doc_id);
    let text_abs = out_paths.root.join(&text_rel);
    fs::write(&text_abs, &doc.body)?;

    docs.insert(doc_id, DocMeta { external_id: doc.id, title: doc.title, url: doc.url, text_path: Some(text_rel) });
    Ok(())
}

#[inline]
fn f32_to_u32(f: f32) -> u32 { f.to_bits() }
#[inline]
fn u32_to_f32(u: u32) -> f32 { f32::from_bits(u) }
