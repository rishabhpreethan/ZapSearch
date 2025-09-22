use core::tokenizer::tokenize;

#[test]
fn it_normalizes_and_stems() {
    let toks = tokenize("Running Runners RUN! The café's menu.");
    let words: Vec<String> = toks.into_iter().map(|(w, _)| w).collect();
    // Stemming to "run" should appear
    assert!(words.contains(&"run".to_string()));
    // Unicode normalization: café -> cafe
    assert!(words.contains(&"cafe".to_string()));
}

#[test]
fn it_filters_stopwords() {
    let toks = tokenize("The quick brown fox and the lazy dog");
    let words: Vec<String> = toks.into_iter().map(|(w, _)| w).collect();
    assert!(!words.contains(&"the".to_string()));
    assert!(!words.contains(&"and".to_string()));
}