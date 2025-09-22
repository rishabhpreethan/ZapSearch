use crate::{DocId, DocMeta, InvertedIndex, Posting, TermId};
use anyhow::Result;
use bincode;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::{create_dir_all, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

#[derive(Debug, Serialize, Deserialize)]
pub struct MetaFile {
    pub num_docs: u32,
    pub created_at: String,
    pub version: u32,
}

pub struct IndexPaths {
    pub root: PathBuf,
}

impl IndexPaths {
    pub fn new<P: AsRef<Path>>(root: P) -> Self {
        Self { root: root.as_ref().to_path_buf() }
    }
    fn dictionary(&self) -> PathBuf { self.root.join("dictionary.bin") }
    fn docs(&self) -> PathBuf { self.root.join("docs.bin") }
    fn meta(&self) -> PathBuf { self.root.join("meta.json") }
    fn postings_dir(&self) -> PathBuf { self.root.join("postings") }
    fn doc_id_map(&self) -> PathBuf { self.root.join("doc_id_map.bin") }
}

pub fn save_dictionary(paths: &IndexPaths, dict: &(HashMap<String, TermId>, Vec<u32>)) -> Result<()> {
    create_dir_all(&paths.root)?;
    let mut f = File::create(paths.dictionary())?;
    let bytes = bincode::serialize(dict)?;
    f.write_all(&bytes)?;
    Ok(())
}

pub fn load_dictionary(paths: &IndexPaths) -> Result<(HashMap<String, TermId>, Vec<u32>)> {
    let mut f = File::open(paths.dictionary())?;
    let mut buf = Vec::new();
    f.read_to_end(&mut buf)?;
    let dict = bincode::deserialize(&buf)?;
    Ok(dict)
}

pub fn save_docs(paths: &IndexPaths, docs: &HashMap<DocId, DocMeta>) -> Result<()> {
    let mut f = File::create(paths.docs())?;
    let bytes = bincode::serialize(docs)?;
    f.write_all(&bytes)?;
    Ok(())
}

pub fn load_docs(paths: &IndexPaths) -> Result<HashMap<DocId, DocMeta>> {
    let mut f = File::open(paths.docs())?;
    let mut buf = Vec::new();
    f.read_to_end(&mut buf)?;
    let docs = bincode::deserialize(&buf)?;
    Ok(docs)
}

pub fn save_postings_for_term(paths: &IndexPaths, term_id: TermId, postings: &Vec<Posting>) -> Result<()> {
    let dir = paths.postings_dir();
    create_dir_all(&dir)?;
    let file = dir.join(format!("{term_id:08}.postings.bin"));
    let mut f = File::create(file)?;
    let bytes = bincode::serialize(postings)?;
    f.write_all(&bytes)?;
    Ok(())
}

pub fn load_postings_for_term(paths: &IndexPaths, term_id: TermId) -> Result<Vec<Posting>> {
    let file = paths.postings_dir().join(format!("{term_id:08}.postings.bin"));
    let mut f = File::open(file)?;
    let mut buf = Vec::new();
    f.read_to_end(&mut buf)?;
    let postings = bincode::deserialize(&buf)?;
    Ok(postings)
}

pub fn save_meta(paths: &IndexPaths, meta: &MetaFile) -> Result<()> {
    create_dir_all(&paths.root)?;
    let mut f = File::create(paths.meta())?;
    let json = serde_json::to_string_pretty(meta)?;
    f.write_all(json.as_bytes())?;
    Ok(())
}

pub fn load_meta(paths: &IndexPaths) -> Result<MetaFile> {
    let mut f = File::open(paths.meta())?;
    let mut buf = String::new();
    f.read_to_string(&mut buf)?;
    let meta: MetaFile = serde_json::from_str(&buf)?;
    Ok(meta)
}

pub fn save_doc_id_map(paths: &IndexPaths, map: &HashMap<String, DocId>) -> Result<()> {
    let mut f = File::create(paths.doc_id_map())?;
    let bytes = bincode::serialize(map)?;
    f.write_all(&bytes)?;
    Ok(())
}

pub fn load_doc_id_map(paths: &IndexPaths) -> Result<HashMap<String, DocId>> {
    let mut f = File::open(paths.doc_id_map())?;
    let mut buf = Vec::new();
    f.read_to_end(&mut buf)?;
    let map = bincode::deserialize(&buf)?;
    Ok(map)
}

/// Load only the header structures required to search: dictionary, df, docs, meta.
pub fn load_index_header(paths: &IndexPaths) -> Result<(HashMap<String, TermId>, Vec<u32>, HashMap<DocId, DocMeta>, MetaFile)> {
    let (dict, df) = load_dictionary(paths)?;
    let docs = load_docs(paths)?;
    let meta = load_meta(paths)?;
    Ok((dict, df, docs, meta))
}
