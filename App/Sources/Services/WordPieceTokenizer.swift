import Foundation

// MARK: - WordPieceTokenizer
//
// Minimal, dependency-free WordPiece (BERT-family) tokenizer for
// `sentence-transformers/all-MiniLM-L6-v2`. MiniLM uses the uncased BERT
// vocabulary, so this mirrors HuggingFace's `BertTokenizer` behaviour closely
// enough for embedding inference:
//
//   1. Basic tokenization — lowercase, strip accents, split on whitespace and
//      punctuation, with CJK characters treated as individual tokens.
//   2. WordPiece — greedy longest-match-first subword segmentation against the
//      vocabulary, emitting `[UNK]` for tokens that cannot be split.
//
// We bundle `bert-vocab.txt` (one token per line, 0-indexed) rather than pull
// in `swift-transformers` — the tokenizer surface MiniLM needs is small and a
// new SPM dependency is a heavier commitment than a ~230 KB vocab file.
//
// This type is pure and `Sendable`: it owns an immutable token→id map and never
// touches global state, so it is safe to share across the embedding actor.

struct WordPieceTokenizer: Sendable {

    // MARK: Special tokens

    /// BERT special tokens. Ids are resolved from the loaded vocab so this stays
    /// correct even if a future vocab reorders them.
    struct SpecialTokens: Sendable {
        let unk: String
        let cls: String
        let sep: String
        let pad: String

        static let bert = SpecialTokens(unk: "[UNK]", cls: "[CLS]", sep: "[SEP]", pad: "[PAD]")
    }

    // MARK: Stored state

    private let vocab: [String: Int]
    private let maxInputCharsPerWord: Int

    /// Token id for `[CLS]`, prepended to every encoded sequence.
    let clsTokenID: Int
    /// Token id for `[SEP]`, appended to every encoded sequence.
    let sepTokenID: Int
    /// Token id for `[PAD]`, used when padding a batch to a common length.
    let padTokenID: Int
    /// Token id for `[UNK]`, emitted for unsplittable WordPiece tokens.
    let unkTokenID: Int

    // MARK: Errors

    enum TokenizerError: LocalizedError {
        case vocabResourceMissing(name: String)
        case vocabEmpty
        case missingSpecialToken(String)

        var errorDescription: String? {
            switch self {
            case let .vocabResourceMissing(name):
                return "WordPiece vocab resource '\(name)' is not bundled. Add bert-vocab.txt to the app target resources."
            case .vocabEmpty:
                return "WordPiece vocab loaded but contained no tokens."
            case let .missingSpecialToken(token):
                return "WordPiece vocab is missing required special token \(token)."
            }
        }
    }

    // MARK: Init

    /// Build a tokenizer from an in-memory token list (token at index `i` has id
    /// `i`). Used directly by tests; the bundle loader funnels through here.
    init(
        vocabTokens: [String],
        special: SpecialTokens = .bert,
        maxInputCharsPerWord: Int = 100
    ) throws {
        guard !vocabTokens.isEmpty else { throw TokenizerError.vocabEmpty }
        var map: [String: Int] = [:]
        map.reserveCapacity(vocabTokens.count)
        for (index, token) in vocabTokens.enumerated() where map[token] == nil {
            map[token] = index
        }
        self.vocab = map
        self.maxInputCharsPerWord = maxInputCharsPerWord

        func require(_ token: String) throws -> Int {
            guard let id = map[token] else { throw TokenizerError.missingSpecialToken(token) }
            return id
        }
        self.clsTokenID = try require(special.cls)
        self.sepTokenID = try require(special.sep)
        self.padTokenID = try require(special.pad)
        self.unkTokenID = try require(special.unk)
    }

    /// Load the bundled `bert-vocab.txt`. One token per line, 0-indexed.
    init(
        bundle: Bundle = .main,
        resource: String = "bert-vocab",
        special: SpecialTokens = .bert
    ) throws {
        guard let url = bundle.url(forResource: resource, withExtension: "txt") else {
            throw TokenizerError.vocabResourceMissing(name: "\(resource).txt")
        }
        let contents = try String(contentsOf: url, encoding: .utf8)
        // Vocab lines are bare tokens; a trailing newline produces an empty
        // final element we must drop, but interior blank lines (if any) are
        // real ids so we only trim the very last empty line.
        var lines = contents.components(separatedBy: "\n")
        if lines.last == "" { lines.removeLast() }
        try self.init(vocabTokens: lines, special: special)
    }

    // MARK: Encoding

    /// Token ids for `text`, wrapped in `[CLS] … [SEP]` and truncated to
    /// `maxLength` (including the two special tokens). MiniLM is trained at 128
    /// tokens; longer chunks are truncated, matching the reference encoder.
    func encode(_ text: String, maxLength: Int = 128) -> [Int] {
        let body = wordPieceTokenIDs(for: text)
        // Reserve two slots for [CLS]/[SEP].
        let bodyBudget = max(0, maxLength - 2)
        let truncated = body.count > bodyBudget ? Array(body.prefix(bodyBudget)) : body
        var ids: [Int] = []
        ids.reserveCapacity(truncated.count + 2)
        ids.append(clsTokenID)
        ids.append(contentsOf: truncated)
        ids.append(sepTokenID)
        return ids
    }

    /// Encode a batch and pad every sequence to the longest in the batch (capped
    /// at `maxLength`). Returns the padded id matrix and a parallel attention
    /// mask (1 for real tokens, 0 for padding) — both are required inputs to the
    /// MiniLM Core ML model.
    func encodeBatch(
        _ texts: [String],
        maxLength: Int = 128
    ) -> (inputIDs: [[Int]], attentionMask: [[Int]]) {
        let encoded = texts.map { encode($0, maxLength: maxLength) }
        let width = encoded.map(\.count).max() ?? 0
        var ids: [[Int]] = []
        var mask: [[Int]] = []
        ids.reserveCapacity(encoded.count)
        mask.reserveCapacity(encoded.count)
        for seq in encoded {
            let padCount = width - seq.count
            ids.append(seq + Array(repeating: padTokenID, count: padCount))
            mask.append(Array(repeating: 1, count: seq.count) + Array(repeating: 0, count: padCount))
        }
        return (ids, mask)
    }

    // MARK: WordPiece

    private func wordPieceTokenIDs(for text: String) -> [Int] {
        var output: [Int] = []
        for token in basicTokenize(text) {
            output.append(contentsOf: wordPiece(token))
        }
        return output
    }

    /// Greedy longest-match-first subword segmentation for a single basic token.
    private func wordPiece(_ token: String) -> [Int] {
        let chars = Array(token)
        if chars.count > maxInputCharsPerWord {
            return [unkTokenID]
        }
        var subTokens: [Int] = []
        var start = 0
        while start < chars.count {
            var end = chars.count
            var matchedID: Int?
            while start < end {
                var piece = String(chars[start..<end])
                if start > 0 { piece = "##" + piece }
                if let id = vocab[piece] {
                    matchedID = id
                    break
                }
                end -= 1
            }
            guard let id = matchedID else {
                // Any unmatchable substring makes the whole word [UNK].
                return [unkTokenID]
            }
            subTokens.append(id)
            start = end
        }
        return subTokens
    }

    // MARK: Basic tokenization

    /// Lowercase, strip accents, then split on whitespace and punctuation. CJK
    /// codepoints are emitted as standalone tokens (matching BERT).
    private func basicTokenize(_ text: String) -> [String] {
        let cleaned = stripAccents(text.lowercased())
        var tokens: [String] = []
        var current = ""

        func flush() {
            if !current.isEmpty {
                tokens.append(current)
                current = ""
            }
        }

        for scalar in cleaned.unicodeScalars {
            if isWhitespace(scalar) {
                flush()
            } else if isPunctuation(scalar) || isCJK(scalar) {
                flush()
                tokens.append(String(scalar))
            } else {
                current.unicodeScalars.append(scalar)
            }
        }
        flush()
        return tokens
    }

    private func stripAccents(_ text: String) -> String {
        // NFD decomposition, then drop combining marks — BERT's accent
        // stripping for the uncased vocab.
        let decomposed = text.decomposedStringWithCanonicalMapping
        var result = ""
        result.reserveCapacity(decomposed.count)
        for scalar in decomposed.unicodeScalars
        where scalar.properties.canonicalCombiningClass == .notReordered {
            result.unicodeScalars.append(scalar)
        }
        return result
    }

    private func isWhitespace(_ s: Unicode.Scalar) -> Bool {
        s == " " || s == "\t" || s == "\n" || s == "\r" || s.properties.isWhitespace
    }

    private func isPunctuation(_ s: Unicode.Scalar) -> Bool {
        let v = s.value
        // BERT treats ASCII non-alphanumeric as punctuation, plus Unicode P*.
        if (v >= 33 && v <= 47) || (v >= 58 && v <= 64) || (v >= 91 && v <= 96) || (v >= 123 && v <= 126) {
            return true
        }
        switch s.properties.generalCategory {
        case .connectorPunctuation, .dashPunctuation, .openPunctuation, .closePunctuation,
             .initialPunctuation, .finalPunctuation, .otherPunctuation:
            return true
        default:
            return false
        }
    }

    private func isCJK(_ s: Unicode.Scalar) -> Bool {
        let v = s.value
        return (0x4E00...0x9FFF).contains(v)
            || (0x3400...0x4DBF).contains(v)
            || (0x20000...0x2A6DF).contains(v)
            || (0x2A700...0x2B73F).contains(v)
            || (0x2B740...0x2B81F).contains(v)
            || (0x2B820...0x2CEAF).contains(v)
            || (0xF900...0xFAFF).contains(v)
            || (0x2F800...0x2FA1F).contains(v)
    }
}
