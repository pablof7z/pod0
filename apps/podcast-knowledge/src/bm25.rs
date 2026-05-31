//! Pure-Rust BM25 (Okapi) ranking over an in-memory corpus.
//!
//! BM25 is the TF-IDF retrieval baseline that replaced the M5.3
//! whole-query substring matcher in the knowledge/RAG search path. It
//! scores a document for a query as:
//!
//! ```text
//! score = Σ_t  IDF(t) · ( tf · (k1 + 1) ) / ( tf + k1 · (1 − b + b · dl/avgdl) )
//! ```
//!
//! with the standard constants `k1 = 1.5`, `b = 0.75`.
//!
//! ## Design choices
//!
//! * **Non-negative IDF.** We use `ln(1 + (N − df + 0.5) / (df + 0.5))`
//!   rather than the textbook `ln((N − df + 0.5) / (df + 0.5))`. The
//!   textbook form goes negative once a term appears in more than half
//!   the documents — trivial on the tiny corpora this kernel sees — and a
//!   negative contribution can rank a matching document *below* a
//!   non-matching one. The `1 +` shift keeps every IDF ≥ 0 so "matches a
//!   term" always beats "matches nothing".
//!
//! * **Query-time index.** No persisted inverted index. The corpus here
//!   is at most a few thousand transcript chunks held in memory, so we
//!   build the document-frequency map and average length in one linear
//!   pass per query. Correctness over throughput (M6.A baseline).
//!
//! * **Bounded score.** Raw BM25 is unbounded, but the search projection
//!   feeds a `relevance_score ∈ [0,1]` UI bar. [`normalize_scores`] maps a
//!   scored result set into `[0,1]` by dividing through the maximum, so
//!   callers can hand the value straight to the projection.
//!
//! No external crates — the whole engine is `std`-only.

/// BM25 term-frequency saturation constant. Higher → term frequency keeps
/// mattering for longer before saturating.
pub const K1: f32 = 1.5;
/// BM25 length-normalisation constant. `0.0` disables length
/// normalisation; `1.0` fully normalises by document length.
pub const B: f32 = 0.75;

/// Tokenise `text` into lowercase alphanumeric terms.
///
/// Splits on any character that is not a Unicode alphanumeric, lowercases
/// each term, and drops empties. This is deliberately simple — no
/// stemming, no stop-word list — because the corpus is short and we want
/// the tokenisation to be auditable and locale-neutral. Multi-byte UTF-8
/// is handled by `char` iteration, so accented terms tokenise cleanly.
pub fn tokenize(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_lowercase())
        .collect()
}

/// A query-time BM25 index over a corpus of pre-tokenised documents.
///
/// Borrows nothing — owns the per-document term lists so it can outlive
/// the source text. Build once per query (or reuse across queries against
/// the same corpus); scoring a document is then O(query terms).
#[derive(Debug, Clone)]
pub struct Bm25Index {
    /// Per-document tokens, indexed by the document's position at build
    /// time. `docs[i]` is the token list for document `i`.
    docs: Vec<Vec<String>>,
    /// Document length (token count) per document.
    doc_len: Vec<f32>,
    /// Document frequency: how many documents contain each term at least
    /// once.
    df: std::collections::HashMap<String, usize>,
    /// Average document length across the corpus. `0.0` for an empty
    /// corpus.
    avgdl: f32,
}

impl Bm25Index {
    /// Build an index from an iterator of documents, each supplied as its
    /// raw text. Documents are tokenised via [`tokenize`] and retain their
    /// input order so [`Bm25Index::score`] / [`Bm25Index::rank`] can refer
    /// to them by index.
    pub fn from_texts<I, S>(texts: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let docs: Vec<Vec<String>> = texts.into_iter().map(|t| tokenize(t.as_ref())).collect();
        Self::from_tokenized(docs)
    }

    /// Build an index from already-tokenised documents. Useful when the
    /// caller tokenises once and feeds the same tokens elsewhere.
    pub fn from_tokenized(docs: Vec<Vec<String>>) -> Self {
        let doc_len: Vec<f32> = docs.iter().map(|d| d.len() as f32).collect();
        let total_len: f32 = doc_len.iter().sum();
        let avgdl = if docs.is_empty() {
            0.0
        } else {
            total_len / docs.len() as f32
        };

        let mut df: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
        for doc in &docs {
            // Count each term once per document (document frequency, not
            // collection frequency), so dedup within the doc first.
            let mut seen = std::collections::HashSet::new();
            for term in doc {
                if seen.insert(term.as_str()) {
                    *df.entry(term.clone()).or_insert(0) += 1;
                }
            }
        }

        Self {
            docs,
            doc_len,
            df,
            avgdl,
        }
    }

    /// Number of documents in the corpus.
    pub fn len(&self) -> usize {
        self.docs.len()
    }

    /// True when the corpus has no documents.
    pub fn is_empty(&self) -> bool {
        self.docs.is_empty()
    }

    /// Non-negative inverse document frequency for `term`.
    ///
    /// `ln(1 + (N − df + 0.5) / (df + 0.5))`. Returns `0.0` for a term that
    /// appears in no document (`df == 0`) or for an empty corpus.
    pub fn idf(&self, term: &str) -> f32 {
        let n = self.docs.len() as f32;
        if n == 0.0 {
            return 0.0;
        }
        let df = *self.df.get(term).unwrap_or(&0) as f32;
        if df == 0.0 {
            return 0.0;
        }
        (1.0 + (n - df + 0.5) / (df + 0.5)).ln()
    }

    /// BM25 score of document `doc_index` against the pre-tokenised
    /// `query_terms`.
    ///
    /// Returns `0.0` when the document index is out of range, the query is
    /// empty, or no query term occurs in the document. The score is the
    /// raw (unbounded) BM25 sum — use [`normalize_scores`] before exposing
    /// it as a `[0,1]` relevance value.
    pub fn score(&self, doc_index: usize, query_terms: &[String]) -> f32 {
        let doc = match self.docs.get(doc_index) {
            Some(d) => d,
            None => return 0.0,
        };
        if doc.is_empty() || query_terms.is_empty() || self.avgdl == 0.0 {
            return 0.0;
        }
        let dl = self.doc_len[doc_index];
        let mut score = 0.0_f32;
        // Score each distinct query term once; a term repeated in the query
        // shouldn't double-count its IDF contribution.
        let mut scored_terms = std::collections::HashSet::new();
        for term in query_terms {
            if !scored_terms.insert(term.as_str()) {
                continue;
            }
            let tf = doc.iter().filter(|t| *t == term).count() as f32;
            if tf == 0.0 {
                continue;
            }
            let idf = self.idf(term);
            let denom = tf + K1 * (1.0 - B + B * dl / self.avgdl);
            score += idf * (tf * (K1 + 1.0)) / denom;
        }
        score
    }

    /// Score every document against `query_terms` and return
    /// `(doc_index, score)` pairs for documents with a strictly positive
    /// score, sorted descending by score (ties broken by ascending index
    /// for determinism).
    ///
    /// Documents that match no query term are dropped — they never reach
    /// the result set, so a no-match query yields an empty ranking.
    pub fn rank(&self, query_terms: &[String]) -> Vec<(usize, f32)> {
        let mut scored: Vec<(usize, f32)> = (0..self.docs.len())
            .map(|i| (i, self.score(i, query_terms)))
            .filter(|(_, s)| *s > 0.0)
            .collect();
        scored.sort_by(|a, b| {
            b.1.partial_cmp(&a.1)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(a.0.cmp(&b.0))
        });
        scored
    }
}

/// Byte offset of the earliest occurrence of any `query_term` within
/// `text`, comparing case-insensitively. Returns `0` when no term is
/// present (callers anchor a snippet at the start in that case).
///
/// Used to anchor a search snippet on the first matched term, replacing
/// the old whole-query `text.to_lowercase().find(&needle)`. Because BM25
/// only scores a document when one of its tokens is present, a ranked
/// document always contains at least one query term as a substring, so
/// this resolves to a real position for any hit.
pub fn first_term_position(text: &str, query_terms: &[String]) -> usize {
    let haystack = text.to_lowercase();
    query_terms
        .iter()
        .filter_map(|term| haystack.find(term.as_str()))
        .min()
        .unwrap_or(0)
}

/// Map raw BM25 scores into `[0,1]` by dividing through the maximum score
/// in the set.
///
/// The largest score becomes `1.0` and the rest scale proportionally. An
/// empty input yields an empty output; a set whose max is `≤ 0` (shouldn't
/// happen after [`Bm25Index::rank`] filters non-positive scores, but guard
/// anyway) is returned unchanged. This is a per-query relative
/// normalisation — it answers "how relevant is this hit *compared to the
/// best hit for this query*", which is exactly what the relevance bar
/// wants.
pub fn normalize_scores(scored: &[(usize, f32)]) -> Vec<(usize, f32)> {
    let max = scored
        .iter()
        .map(|(_, s)| *s)
        .fold(f32::MIN, f32::max);
    if max <= 0.0 {
        return scored.to_vec();
    }
    scored
        .iter()
        .map(|(i, s)| (*i, (s / max).clamp(0.0, 1.0)))
        .collect()
}

#[cfg(test)]
#[path = "bm25_tests.rs"]
mod tests;
