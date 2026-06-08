import XCTest
@testable import Podcastr

final class WordPieceTokenizerTests: XCTestCase {

    /// Small fixture vocab exercising special tokens, full words, subword
    /// continuations (`##`), and a token that forces `[UNK]`.
    private func makeTokenizer() throws -> WordPieceTokenizer {
        // Index == id. Special tokens first, mirroring BERT layout loosely.
        let tokens = [
            "[PAD]",        // 0
            "[UNK]",        // 1
            "[CLS]",        // 2
            "[SEP]",        // 3
            "the",          // 4
            "keto",         // 5
            "diet",         // 6
            "insulin",      // 7
            "play",         // 8
            "##ing",        // 9
            "##s",          // 10
            ".",            // 11
            "podcast",      // 12
        ]
        return try WordPieceTokenizer(vocabTokens: tokens)
    }

    func testSpecialTokenIDsResolveFromVocab() throws {
        let tok = try makeTokenizer()
        XCTAssertEqual(tok.padTokenID, 0)
        XCTAssertEqual(tok.unkTokenID, 1)
        XCTAssertEqual(tok.clsTokenID, 2)
        XCTAssertEqual(tok.sepTokenID, 3)
    }

    func testEncodeWrapsWithClsAndSep() throws {
        let tok = try makeTokenizer()
        let ids = tok.encode("the keto diet")
        XCTAssertEqual(ids.first, tok.clsTokenID)
        XCTAssertEqual(ids.last, tok.sepTokenID)
        // [CLS] the keto diet [SEP]
        XCTAssertEqual(ids, [2, 4, 5, 6, 3])
    }

    func testWordPieceSubwordContinuation() throws {
        let tok = try makeTokenizer()
        // "playing" -> "play" + "##ing"
        let ids = tok.encode("playing")
        XCTAssertEqual(ids, [tok.clsTokenID, 8, 9, tok.sepTokenID])
    }

    func testUnknownWordBecomesUnk() throws {
        let tok = try makeTokenizer()
        // "zzzz" can't be segmented against the fixture vocab.
        let ids = tok.encode("zzzz")
        XCTAssertEqual(ids, [tok.clsTokenID, tok.unkTokenID, tok.sepTokenID])
    }

    func testLowercasingIsApplied() throws {
        let tok = try makeTokenizer()
        XCTAssertEqual(tok.encode("KETO"), tok.encode("keto"))
    }

    func testPunctuationSplitsIntoOwnToken() throws {
        let tok = try makeTokenizer()
        // "diet." -> "diet" "."
        let ids = tok.encode("diet.")
        XCTAssertEqual(ids, [tok.clsTokenID, 6, 11, tok.sepTokenID])
    }

    func testMaxLengthTruncatesBodyButKeepsSpecials() throws {
        let tok = try makeTokenizer()
        // maxLength 3 leaves room for [CLS] + 1 body + [SEP].
        let ids = tok.encode("the keto diet", maxLength: 3)
        XCTAssertEqual(ids.count, 3)
        XCTAssertEqual(ids.first, tok.clsTokenID)
        XCTAssertEqual(ids.last, tok.sepTokenID)
    }

    func testBatchEncodingPadsAndMasks() throws {
        let tok = try makeTokenizer()
        let (ids, mask) = tok.encodeBatch(["the", "the keto diet"])
        // Both rows pad to the longest (5: [CLS] the keto diet [SEP]).
        XCTAssertEqual(ids[0].count, ids[1].count)
        XCTAssertEqual(ids[0].count, 5)
        // Row 0 ("the") has 3 real tokens then 2 pads.
        XCTAssertEqual(mask[0], [1, 1, 1, 0, 0])
        XCTAssertEqual(ids[0].suffix(2), [tok.padTokenID, tok.padTokenID])
        // Row 1 is fully real.
        XCTAssertEqual(mask[1], [1, 1, 1, 1, 1])
    }

    func testEmptyVocabThrows() {
        XCTAssertThrowsError(try WordPieceTokenizer(vocabTokens: []))
    }

    func testMissingSpecialTokenThrows() {
        // No [CLS] etc.
        XCTAssertThrowsError(try WordPieceTokenizer(vocabTokens: ["the", "diet"]))
    }

    /// Sanity check against the REAL bundled BERT vocab if present in the test
    /// bundle: the standard `all-MiniLM-L6-v2` vocab pins these canonical ids.
    func testBundledVocabHasCanonicalBertIDs() throws {
        guard let url = Bundle(for: Self.self).url(forResource: "bert-vocab", withExtension: "txt")
            ?? Bundle.main.url(forResource: "bert-vocab", withExtension: "txt") else {
            throw XCTSkip("bert-vocab.txt not in test bundle")
        }
        let lines = try String(contentsOf: url, encoding: .utf8)
            .components(separatedBy: "\n")
            .filter { !$0.isEmpty || true } // keep positions; dropLast handled below
        var tokens = lines
        if tokens.last == "" { tokens.removeLast() }
        let tok = try WordPieceTokenizer(vocabTokens: tokens)
        XCTAssertEqual(tok.padTokenID, 0)
        XCTAssertEqual(tok.unkTokenID, 100)
        XCTAssertEqual(tok.clsTokenID, 101)
        XCTAssertEqual(tok.sepTokenID, 102)
    }
}
