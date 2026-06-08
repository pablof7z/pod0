import Foundation
import os.log

#if os(iOS)
import CoreML

// MARK: - CoreMLEmbeddingProvider
//
// On-device sentence embeddings for iPhone RAG (issue #236) using a Core ML
// build of `sentence-transformers/all-MiniLM-L6-v2`. Produces 384-dim vectors
// on the Neural Engine (A12+), eliminating the OpenRouter round-trip for
// embedding the knowledge pipeline's chunks.
//
// Model I/O contract (standard `coremltools` BERT conversion):
//   inputs : `input_ids`, `attention_mask` — MLMultiArray [1, seqLen], Int32
//   output : either a pooled sentence vector (`sentence_embedding` /
//            `pooler_output`), or per-token hidden states
//            (`last_hidden_state` / `token_embeddings`) shaped
//            [1, seqLen, 384] that we mean-pool over the attention mask.
// We resolve output names from the loaded model's `modelDescription` so the
// provider tolerates either conversion flavour. Sentence vectors are
// L2-normalized so cosine similarity in the vector index reduces to a dot
// product (matching how the cloud embeddings are stored).
//
// Readiness: the `.mlpackage` is a post-install download (~23 MB), not bundled
// in the IPA — same pattern as Gemma. `isReady` reflects on-disk presence and
// a successful compile/load. When not ready, `embed` throws `.modelNotReady`
// so the caller can fall back to the cloud provider (see `LocalEmbeddingsClient`).
//
// `@available(iOS 16, *)`: we target the modern `MLModel.compileModel(at:)`
// async API and `MLModelConfiguration.computeUnits = .all` Neural Engine path.

@available(iOS 16, *)
actor CoreMLEmbeddingProvider: EmbeddingProvider {

    // MARK: Logger

    private static let logger = Logger.app("CoreMLEmbeddingProvider")

    // MARK: Config

    /// Catalog id whose `.mlpackage` this provider loads.
    private let modelID: String
    /// Resolved `.mlpackage` location on disk.
    private let modelURL: URL
    private let tokenizer: WordPieceTokenizer
    private let maxSequenceLength: Int

    nonisolated let dimensions: Int

    // MARK: Lazy model state

    /// Loaded model, compiled from the `.mlpackage` on first `embed`. Held so we
    /// don't recompile per call. `nil` until first successful load.
    private var model: MLModel?
    /// Resolved I/O feature names, discovered from `modelDescription` at load.
    private var io: ResolvedIO?

    private struct ResolvedIO {
        let inputIDsName: String
        let attentionMaskName: String
        /// The output we read embeddings from.
        let outputName: String
        /// Whether `outputName` is already a pooled sentence vector (rank-1 /
        /// [1, D]) vs. per-token hidden states ([1, T, D]) needing mean-pool.
        let isPooled: Bool
    }

    // MARK: Init

    /// - Parameters:
    ///   - modelID: catalog id (default `all-minilm-l6-v2`).
    ///   - modelURL: explicit `.mlpackage` location. Defaults to the canonical
    ///     download destination; tests pass a fixture path.
    ///   - tokenizer: WordPiece tokenizer (default loads the bundled BERT vocab).
    init(
        modelID: String = MiniLMEmbedding.modelID,
        modelURL: URL? = nil,
        tokenizer: WordPieceTokenizer? = nil,
        dimensions: Int = MiniLMEmbedding.dimensions,
        maxSequenceLength: Int = MiniLMEmbedding.maxSequenceLength
    ) throws {
        self.modelID = modelID
        self.modelURL = modelURL ?? DownloadCapability.embeddingModelFileURL(for: modelID)
        self.dimensions = dimensions
        self.maxSequenceLength = maxSequenceLength
        if let tokenizer {
            self.tokenizer = tokenizer
        } else {
            do {
                self.tokenizer = try WordPieceTokenizer()
            } catch {
                throw EmbeddingProviderError.tokenizerUnavailable(underlying: error.localizedDescription)
            }
        }
    }

    // MARK: EmbeddingProvider

    nonisolated var isReady: Bool {
        // Presence of the `.mlpackage` on disk. We deliberately don't try to
        // compile here (sync + expensive); a load failure on `embed` surfaces
        // as `.modelLoadFailed` and the caller falls back.
        FileManager.default.fileExists(atPath: modelURL.path)
    }

    func embed(_ texts: [String]) async throws -> [[Float]] {
        guard !texts.isEmpty else { return [] }
        guard isReady else { throw EmbeddingProviderError.modelNotReady }

        let (model, io) = try await loadedModel()

        var output: [[Float]] = []
        output.reserveCapacity(texts.count)
        // MiniLM is a small model; we run one text per prediction to keep the
        // MLMultiArray shapes fixed and avoid ragged-batch padding cost. The
        // outer batching/throughput concern is the caller's (VectorIndex upserts
        // in chunk batches; each chunk is one short passage).
        for text in texts {
            let vector = try predict(text: text, model: model, io: io)
            guard vector.count == dimensions else {
                throw EmbeddingProviderError.dimensionMismatch(expected: dimensions, got: vector.count)
            }
            output.append(vector)
        }
        return output
    }

    // MARK: Model loading

    private func loadedModel() async throws -> (MLModel, ResolvedIO) {
        if let model, let io { return (model, io) }

        let config = MLModelConfiguration()
        config.computeUnits = .all // Prefer the Neural Engine; CPU/GPU fallback.

        let loaded: MLModel
        do {
            // `.mlpackage` must be compiled to `.mlmodelc` before loading.
            let compiledURL = try await MLModel.compileModel(at: modelURL)
            loaded = try MLModel(contentsOf: compiledURL, configuration: config)
        } catch {
            throw EmbeddingProviderError.modelLoadFailed(underlying: error.localizedDescription)
        }

        let resolved = try Self.resolveIO(from: loaded, dimensions: dimensions)
        self.model = loaded
        self.io = resolved
        Self.logger.info(
            "loaded MiniLM embedder ids=\(resolved.inputIDsName, privacy: .public) mask=\(resolved.attentionMaskName, privacy: .public) out=\(resolved.outputName, privacy: .public) pooled=\(resolved.isPooled, privacy: .public)"
        )
        return (loaded, resolved)
    }

    /// Resolve input/output feature names from the model description, tolerating
    /// both pooled and per-token conversion flavours.
    private static func resolveIO(from model: MLModel, dimensions: Int) throws -> ResolvedIO {
        let desc = model.modelDescription
        let inputNames = Set(desc.inputDescriptionsByName.keys)

        func pickInput(_ candidates: [String]) -> String? {
            candidates.first(where: { inputNames.contains($0) })
        }
        guard let idsName = pickInput(["input_ids", "input_ids_1", "tokens"]) else {
            throw EmbeddingProviderError.inferenceFailed(detail: "model has no recognizable input_ids feature; inputs=\(inputNames)")
        }
        guard let maskName = pickInput(["attention_mask", "attention_mask_1", "mask"]) else {
            throw EmbeddingProviderError.inferenceFailed(detail: "model has no recognizable attention_mask feature; inputs=\(inputNames)")
        }

        // Prefer a named pooled sentence output if present; else fall back to a
        // per-token hidden-state output that we mean-pool.
        let outputsByName = desc.outputDescriptionsByName
        let pooledCandidates = ["sentence_embedding", "pooler_output", "embeddings", "pooled_output"]
        let tokenCandidates = ["last_hidden_state", "token_embeddings", "hidden_states", "output", "var_"]

        if let pooled = pooledCandidates.first(where: { outputsByName[$0] != nil }) {
            return ResolvedIO(inputIDsName: idsName, attentionMaskName: maskName, outputName: pooled, isPooled: true)
        }
        if let token = tokenCandidates.first(where: { outputsByName[$0] != nil }) {
            return ResolvedIO(inputIDsName: idsName, attentionMaskName: maskName, outputName: token, isPooled: false)
        }
        // Last resort: take the single multiarray output if there's exactly one.
        let multiArrayOutputs = outputsByName.filter { $0.value.type == .multiArray }
        if let only = multiArrayOutputs.keys.first, multiArrayOutputs.count == 1 {
            // Infer pooled-ness from the declared shape rank when available.
            let shape = outputsByName[only]?.multiArrayConstraint?.shape ?? []
            let isPooled = shape.count <= 2
            return ResolvedIO(inputIDsName: idsName, attentionMaskName: maskName, outputName: only, isPooled: isPooled)
        }
        throw EmbeddingProviderError.inferenceFailed(detail: "no usable embedding output; outputs=\(Set(outputsByName.keys))")
    }

    // MARK: Inference

    private func predict(text: String, model: MLModel, io: ResolvedIO) throws -> [Float] {
        let ids = tokenizer.encode(text, maxLength: maxSequenceLength)
        let seqLen = ids.count
        let mask = Array(repeating: 1, count: seqLen)

        let idsArray = try multiArray(from: ids)
        let maskArray = try multiArray(from: mask)

        let provider = try MLDictionaryFeatureProvider(dictionary: [
            io.inputIDsName: MLFeatureValue(multiArray: idsArray),
            io.attentionMaskName: MLFeatureValue(multiArray: maskArray),
        ])

        let result: MLFeatureProvider
        do {
            result = try model.prediction(from: provider)
        } catch {
            throw EmbeddingProviderError.inferenceFailed(detail: error.localizedDescription)
        }

        guard let value = result.featureValue(for: io.outputName)?.multiArrayValue else {
            throw EmbeddingProviderError.inferenceFailed(detail: "missing output \(io.outputName)")
        }

        let pooled: [Float]
        if io.isPooled {
            pooled = Self.readPooled(value, dimensions: dimensions)
        } else {
            pooled = Self.meanPool(value, attentionMask: mask, dimensions: dimensions)
        }
        return Self.l2Normalize(pooled)
    }

    /// Shape `[seqLen]` Int32 multiarray with a leading batch dim of 1: `[1, seqLen]`.
    private func multiArray(from ints: [Int]) throws -> MLMultiArray {
        let array = try MLMultiArray(shape: [1, NSNumber(value: ints.count)], dataType: .int32)
        for (i, v) in ints.enumerated() {
            array[[0, NSNumber(value: i)]] = NSNumber(value: Int32(v))
        }
        return array
    }

    // MARK: Pooling

    /// Read a pre-pooled `[1, D]` (or `[D]`) sentence vector.
    private static func readPooled(_ array: MLMultiArray, dimensions: Int) -> [Float] {
        let count = min(array.count, dimensions)
        var out = [Float](repeating: 0, count: dimensions)
        for i in 0..<count {
            out[i] = array[i].floatValue
        }
        return out
    }

    /// Mean-pool per-token hidden states `[1, T, D]` over the attention mask,
    /// the standard sentence-transformers pooling for MiniLM.
    private static func meanPool(_ array: MLMultiArray, attentionMask: [Int], dimensions: Int) -> [Float] {
        // Shape is [1, T, D]; strides let us index without assuming contiguity.
        let shape = array.shape.map(\.intValue)
        let strides = array.strides.map(\.intValue)
        guard shape.count == 3 else {
            // Unexpected rank — fall back to treating it as already pooled.
            return readPooled(array, dimensions: dimensions)
        }
        let tokenCount = shape[1]
        let dim = min(shape[2], dimensions)
        var sum = [Float](repeating: 0, count: dimensions)
        var counted = 0
        let ptr = array.dataPointer.bindMemory(to: Float.self, capacity: array.count)
        let isFloat32 = array.dataType == .float32
        for t in 0..<tokenCount {
            if t < attentionMask.count, attentionMask[t] == 0 { continue }
            counted += 1
            let base = strides[0] * 0 + strides[1] * t
            for d in 0..<dim {
                let idx = base + strides[2] * d
                let v: Float = isFloat32 ? ptr[idx] : array[idx].floatValue
                sum[d] += v
            }
        }
        if counted > 0 {
            let inv = 1.0 / Float(counted)
            for d in 0..<dimensions { sum[d] *= inv }
        }
        return sum
    }

    /// L2-normalize so cosine similarity reduces to a dot product.
    private static func l2Normalize(_ vector: [Float]) -> [Float] {
        var norm: Float = 0
        for v in vector { norm += v * v }
        norm = norm.squareRoot()
        guard norm > 1e-12 else { return vector }
        let inv = 1.0 / norm
        return vector.map { $0 * inv }
    }
}

#endif
