use lazy_static::lazy_static;
use regex::Regex;
use rust_stemmers::{Algorithm, Stemmer};
use unicode_normalization::UnicodeNormalization;
use std::collections::HashSet;

lazy_static! {
    static ref RE: Regex = Regex::new(r"(?u)\p{L}[\p{L}\p{N}_']*").expect("valid regex");
    static ref STEMMER: Stemmer = Stemmer::create(Algorithm::English);
    static ref STOPWORDS: HashSet<&'static str> = {
        let words: &[&str] = &[
            "a","about","above","after","again","against","all","am","an","and","any","are","aren't","as","at",
            "be","because","been","before","being","below","between","both","but","by",
            "can","can't","cannot","could","couldn't",
            "did","didn't","do","does","doesn't","doing","don't","down","during",
            "each","few","for","from","further",
            "had","hadn't","has","hasn't","have","haven't","having","he","he'd","he'll","he's","her","here","here's","hers","herself","him","himself","his","how","how's",
            "i","i'd","i'll","i'm","i've","if","in","into","is","isn't","it","it's","its","itself",
            "let's","me","more","most","mustn't","my","myself",
            "no","nor","not","of","off","on","once","only","or","other","ought","our","ours","ourselves","out","over","own",
            "same","she","she'd","she'll","she's","should","shouldn't","so","some","such",
            "than","that","that's","the","their","theirs","them","themselves","then","there","there's","these","they","they'd","they'll","they're","they've","this","those","through","to","too",
            "under","until","up","very",
            "was","wasn't","we","we'd","we'll","we're","we've","were","weren't","what","what's","when","when's","where","where's","which","while","who","who's","whom","why","why's","with","won't","would","wouldn't",
            "you","you'd","you'll","you're","you've","your","yours","yourself","yourselves"
        ];
        words.iter().copied().collect()
    };
}

fn is_stopword(token: &str) -> bool { STOPWORDS.contains(token) }

/// Tokenize text into (term, position) using NFKC normalization, lowercase, stopword removal, and stemming.
pub fn tokenize(text: &str) -> Vec<(String, usize)> {
    let normalized = text.nfkc().collect::<String>().to_lowercase();
    let mut tokens = Vec::new();
    for (pos, mat) in RE.find_iter(&normalized).enumerate() {
        let token = mat.as_str();
        if is_stopword(token) { continue; }
        let stem = STEMMER.stem(token).to_string();
        tokens.push((stem, pos));
    }
    tokens
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_tokenize() {
        let t = tokenize("Running, runner's run!");
        assert!(t.iter().any(|(w, _)| w == "run"));
    }
}
