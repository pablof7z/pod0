use super::*;

/// The substring matcher the BM25 engine replaces: case-insensitive
/// contiguous-substring match of the *whole query*. Reproduced inline so
/// the "BM25 beats substring" comparison stays self-contained after the
/// production substring code is deleted.
fn substring_matches(text: &str, query: &str) -> bool {
    text.to_lowercase().contains(&query.to_lowercase())
}

#[test]
fn tokenize_lowercases_and_splits_on_punctuation() {
    assert_eq!(
        tokenize("Distributed, Consensus! protocols."),
        vec!["distributed", "consensus", "protocols"]
    );
    // Multi-byte UTF-8 stays intact and lowercases.
    assert_eq!(tokenize("Café—München"), vec!["café", "münchen"]);
    // Empty / punctuation-only → no tokens.
    assert!(tokenize("   ").is_empty());
    assert!(tokenize("!!!,.").is_empty());
}

#[test]
fn idf_is_non_negative_even_for_common_terms() {
    // "the" appears in all 3 docs (df == N). The textbook IDF would go
    // negative here; the +1 shift must keep it ≥ 0.
    let idx = Bm25Index::from_texts([
        "the quick brown fox",
        "the lazy dog",
        "the end",
    ]);
    assert!(idx.idf("the") >= 0.0, "common-term IDF must be non-negative");
    // A rarer term must out-weigh the ubiquitous one.
    assert!(idx.idf("fox") > idx.idf("the"));
    // A term absent from the corpus contributes nothing.
    assert_eq!(idx.idf("absent"), 0.0);
}

#[test]
fn empty_corpus_and_empty_query_score_zero() {
    let empty = Bm25Index::from_texts(Vec::<&str>::new());
    assert!(empty.is_empty());
    assert_eq!(empty.score(0, &tokenize("anything")), 0.0);

    let idx = Bm25Index::from_texts(["some content here"]);
    // Empty query → 0.
    assert_eq!(idx.score(0, &[]), 0.0);
    // Out-of-range doc index → 0, no panic.
    assert_eq!(idx.score(99, &tokenize("content")), 0.0);
}

#[test]
fn rank_drops_non_matching_documents() {
    let idx = Bm25Index::from_texts([
        "feline behavior research",       // 0: no match
        "quantum entanglement explained", // 1: match
        "about cats and dogs",            // 2: no match
    ]);
    let ranked = idx.rank(&tokenize("quantum"));
    assert_eq!(ranked.len(), 1, "only the matching doc survives");
    assert_eq!(ranked[0].0, 1);

    // A query no document matches → empty ranking (drives
    // `no_match_returns_empty` at the call sites).
    assert!(idx.rank(&tokenize("zebra")).is_empty());
}

#[test]
fn higher_term_frequency_ranks_higher() {
    let idx = Bm25Index::from_texts([
        "consensus consensus consensus among nodes", // 0: tf=3
        "consensus among distributed nodes",          // 1: tf=1
    ]);
    let ranked = idx.rank(&tokenize("consensus"));
    assert_eq!(ranked.len(), 2);
    assert_eq!(ranked[0].0, 0, "the doc with more occurrences ranks first");
    assert!(ranked[0].1 > ranked[1].1);
}

#[test]
fn multi_term_query_rewards_documents_matching_more_terms() {
    let idx = Bm25Index::from_texts([
        "distributed consensus in raft", // 0: matches both query terms
        "distributed systems overview",  // 1: matches one query term
        "a cooking podcast episode",     // 2: matches neither
    ]);
    let ranked = idx.rank(&tokenize("distributed consensus"));
    assert_eq!(ranked.len(), 2, "the non-matching doc is dropped");
    assert_eq!(ranked[0].0, 0, "matching both terms outranks matching one");
    assert!(ranked[0].1 > ranked[1].1);
}

#[test]
fn bm25_finds_what_substring_match_misses() {
    // THE headline test (task step 6). Query terms are present in the
    // document but NOT as a contiguous substring — the words appear in a
    // different order with another word between them.
    let text = "consensus among distributed nodes";
    let query = "distributed consensus";

    // Baseline lexical substring matcher: the whole query is not a
    // contiguous substring → no match at all.
    assert!(
        !substring_matches(text, query),
        "substring baseline must miss the reordered phrase"
    );

    // BM25 tokenises the query and scores term-by-term → finds the doc.
    let idx = Bm25Index::from_texts([text]);
    let ranked = idx.rank(&tokenize(query));
    assert_eq!(ranked.len(), 1, "BM25 finds the doc substring matching misses");
    assert!(ranked[0].1 > 0.0);
}

#[test]
fn bm25_ranks_more_relevant_doc_above_incidental_mention() {
    // Substring match treats every hit equally (early-position heuristic
    // aside); BM25 uses TF-IDF so a focused doc beats an incidental one.
    let idx = Bm25Index::from_texts([
        // 0: long doc, one incidental mention of "bitcoin" buried in noise.
        "today we talk about the weather and gardening and then briefly bitcoin \
         before returning to gardening tips and seasonal planting advice for spring",
        // 1: short, focused doc — "bitcoin" is the topic.
        "bitcoin halving explained",
    ]);
    let ranked = idx.rank(&tokenize("bitcoin"));
    assert_eq!(ranked.len(), 2);
    // Length normalisation (b=0.75): the short focused doc outranks the
    // long doc where the term is diluted.
    assert_eq!(ranked[0].0, 1, "focused short doc beats diluted long doc");
}

#[test]
fn normalize_scores_maps_top_to_one_and_bounds_range() {
    let idx = Bm25Index::from_texts([
        "consensus consensus consensus nodes",
        "consensus nodes",
    ]);
    let ranked = idx.rank(&tokenize("consensus"));
    let normed = normalize_scores(&ranked);
    assert_eq!(normed.len(), 2);
    // Top score normalises to exactly 1.0.
    assert!((normed[0].1 - 1.0).abs() < 1e-6);
    // Every normalised score is within [0,1].
    assert!(normed.iter().all(|(_, s)| *s >= 0.0 && *s <= 1.0));
    // Order is preserved and strictly descending here.
    assert!(normed[0].1 > normed[1].1);
}

#[test]
fn normalize_scores_handles_empty_input() {
    assert!(normalize_scores(&[]).is_empty());
}

#[test]
fn first_term_position_finds_earliest_match() {
    let text = "consensus among distributed nodes";
    // "distributed" is at byte 15, "consensus" at byte 0 → earliest wins.
    let terms = tokenize("distributed consensus");
    assert_eq!(first_term_position(text, &terms), 0);

    // When only the later term is present, anchor on it.
    let terms2 = tokenize("distributed missing");
    assert_eq!(first_term_position(text, &terms2), "consensus among ".len());

    // No term present → anchor at the start (0).
    assert_eq!(first_term_position(text, &tokenize("zebra")), 0);
    // Case-insensitive.
    assert_eq!(first_term_position("Distributed Systems", &tokenize("systems")), "Distributed ".len());
}
