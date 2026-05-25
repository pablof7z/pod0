// TokenSwapTests.swift — Unit tests for apply-token-swap rewriter logic.
//
// Fixture 1: AppStateStore → KernelModel (identifier rename)
// Fixture 2: AudioEngine.shared.play(episode) → model.playEpisode(episode.id)
// Fixture 3: RAGService.shared.search(q) → model.searchTranscripts(q)

import XCTest
import SwiftParser
import SwiftSyntax

// Re-import the types from apply-token-swap source.
// Because the executableTarget doesn't expose a library, we duplicate the
// minimal types needed for testing here.  The real tool's logic is tested
// end-to-end by running the binary; these tests cover the rewriter core.

// ── Minimal rule types (mirror apply-token-swap internals) ─────────────────

struct SwapRule {
    enum Kind: String { case identifier, member_call, import_remove }
    let kind: Kind
    let from: String
    let to: String
    let argTransform: String?

    init(kind: Kind, from: String, to: String, argTransform: String? = nil) {
        self.kind = kind
        self.from = from
        self.to = to
        self.argTransform = argTransform
    }
}

// ── Rewriter (mirrored from apply-token-swap/main.swift) ──────────────────

final class TokenSwapRewriter: SyntaxRewriter {
    let rules: [SwapRule]

    init(rules: [SwapRule]) {
        self.rules = rules
    }

    override func visit(_ node: CodeBlockItemListSyntax) -> CodeBlockItemListSyntax {
        let visited = super.visit(node)
        let filtered = visited.filter { item in
            guard let importDecl = item.item.as(ImportDeclSyntax.self) else { return true }
            let moduleName = importDecl.path.map { $0.name.text }.joined(separator: ".")
            return !rules.contains { $0.kind == .import_remove && $0.from == moduleName }
        }
        return filtered
    }

    override func visit(_ token: TokenSyntax) -> TokenSyntax {
        guard case .identifier(let text) = token.tokenKind else { return token }
        for rule in rules where rule.kind == .identifier && rule.from == text {
            return token.with(\.tokenKind, .identifier(rule.to))
        }
        return token
    }

    override func visit(_ node: FunctionCallExprSyntax) -> ExprSyntax {
        guard let rewritten = rewriteMemberCall(node) else {
            return ExprSyntax(node)
        }
        return ExprSyntax(rewritten)
    }

    private func rewriteMemberCall(_ node: FunctionCallExprSyntax) -> FunctionCallExprSyntax? {
        guard let outerMember = node.calledExpression.as(MemberAccessExprSyntax.self),
              let innerMember = outerMember.base?.as(MemberAccessExprSyntax.self),
              let baseExpr = innerMember.base?.as(DeclReferenceExprSyntax.self) else {
            return nil
        }

        let baseName = baseExpr.baseName.text
        let sharedName = innerMember.declName.baseName.text
        let methodName = outerMember.declName.baseName.text

        guard sharedName == "shared" else { return nil }
        let fromPattern = "\(baseName).shared.\(methodName)"

        for rule in rules where rule.kind == .member_call && rule.from == fromPattern {
            let modelBase = DeclReferenceExprSyntax(baseName: .identifier("model"))
            let newCallee = MemberAccessExprSyntax(
                base: ExprSyntax(modelBase),
                period: .periodToken(),
                declName: DeclReferenceExprSyntax(
                    baseName: .identifier(rule.to.components(separatedBy: ".").last ?? rule.to)
                )
            )

            var newArgs = node.arguments
            if rule.argTransform == "first_arg_dot_id",
               var firstArg = newArgs.first {
                let argExpr = firstArg.expression
                let wrapped = MemberAccessExprSyntax(
                    base: argExpr,
                    period: .periodToken(),
                    declName: DeclReferenceExprSyntax(baseName: .identifier("id"))
                )
                firstArg = firstArg.with(\.expression, ExprSyntax(wrapped))
                var argList = Array(newArgs)
                argList[0] = firstArg
                newArgs = LabeledExprListSyntax(argList)
            }

            return node
                .with(\.calledExpression, ExprSyntax(newCallee))
                .with(\.arguments, newArgs)
        }
        return nil
    }
}

// ── Helper ────────────────────────────────────────────────────────────────────

func applyRules(_ source: String, rules: [SwapRule]) -> String {
    let tree = Parser.parse(source: source)
    let rewriter = TokenSwapRewriter(rules: rules)
    return rewriter.visit(tree).description
}

// ── Tests ─────────────────────────────────────────────────────────────────────

final class TokenSwapTests: XCTestCase {

    // Fixture 1: AppStateStore → KernelModel (identifier rename)
    func testIdentifierRename_AppStateStore_to_KernelModel() {
        let source = """
        @EnvironmentObject var store: AppStateStore

        func doSomething(store: AppStateStore) {
            let s: AppStateStore = store
        }
        """
        let rules = [
            SwapRule(kind: .identifier, from: "AppStateStore", to: "KernelModel")
        ]
        let result = applyRules(source, rules: rules)
        XCTAssertFalse(result.contains("AppStateStore"),
            "AppStateStore should have been renamed to KernelModel")
        XCTAssertTrue(result.contains("KernelModel"),
            "KernelModel should appear in the result")
        // Ensure it's not a substring collision — only full identifier matches
        let count = result.components(separatedBy: "KernelModel").count - 1
        XCTAssertEqual(count, 3,
            "All 3 occurrences of AppStateStore should be renamed")
    }

    // Fixture 2: AudioEngine.shared.play(episode) → model.playEpisode(episode.id)
    func testMemberCallRewrite_AudioEngine_play() {
        let source = """
        func startPlayback() {
            AudioEngine.shared.play(episode)
        }
        """
        let rules = [
            SwapRule(
                kind: .member_call,
                from: "AudioEngine.shared.play",
                to: "model.playEpisode",
                argTransform: "first_arg_dot_id"
            )
        ]
        let result = applyRules(source, rules: rules)
        XCTAssertFalse(result.contains("AudioEngine"),
            "AudioEngine should have been removed")
        XCTAssertTrue(result.contains("model.playEpisode"),
            "model.playEpisode should appear in the result")
        XCTAssertTrue(result.contains("episode.id"),
            "argument should have .id appended")
    }

    // Fixture 3: RAGService.shared.search(q) → model.searchTranscripts(q)
    func testMemberCallRewrite_RAGService_search() {
        let source = """
        func runSearch(query: String) {
            let results = RAGService.shared.search(query)
        }
        """
        let rules = [
            SwapRule(
                kind: .member_call,
                from: "RAGService.shared.search",
                to: "model.searchTranscripts"
            )
        ]
        let result = applyRules(source, rules: rules)
        XCTAssertFalse(result.contains("RAGService"),
            "RAGService should have been removed")
        XCTAssertTrue(result.contains("model.searchTranscripts"),
            "model.searchTranscripts should appear in the result")
        // Argument should be passed through unchanged
        XCTAssertTrue(result.contains("query"),
            "original argument should be preserved")
    }

    // Bonus: import removal
    func testImportRemoval() {
        let source = """
        import SwiftUI
        import AudioEngine

        struct MyView: View {
            var body: some View { Text("hello") }
        }
        """
        let rules = [
            SwapRule(kind: .import_remove, from: "AudioEngine", to: "")
        ]
        let result = applyRules(source, rules: rules)
        XCTAssertFalse(result.contains("import AudioEngine"),
            "import AudioEngine should have been removed")
        XCTAssertTrue(result.contains("import SwiftUI"),
            "import SwiftUI should be preserved")
        XCTAssertTrue(result.contains("MyView"),
            "View struct should be preserved")
    }
}
