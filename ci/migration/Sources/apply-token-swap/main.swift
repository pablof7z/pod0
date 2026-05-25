// apply-token-swap — AST-level token-swap CLI for the NMP migration.
//
// Usage:
//   apply-token-swap <file.swift> [--toml <path/to/token-swap.toml>]
//   apply-token-swap --help
//
// Reads ci/migration/token-swap.toml, parses it into a SwapRule list, then
// applies each rule to the Swift file's syntax tree.  The file is written in
// place.  All edits are AST-level (identifier nodes, import declarations,
// member-access call sites) — never raw string replacement.

import Foundation
import SwiftParser
import SwiftSyntax

// ── Entry point ──────────────────────────────────────────────────────────────

func main() throws {
    let args = Array(CommandLine.arguments.dropFirst())

    if args.isEmpty || args.contains("--help") || args.contains("-h") {
        printHelp()
        exit(args.isEmpty ? 1 : 0)
    }

    var filePath: String? = nil
    var tomlPath: String? = nil
    var i = 0
    while i < args.count {
        switch args[i] {
        case "--toml":
            i += 1
            guard i < args.count else {
                fputs("Error: --toml requires a path argument\n", stderr)
                exit(1)
            }
            tomlPath = args[i]
        default:
            if filePath == nil {
                filePath = args[i]
            } else {
                fputs("Error: unexpected argument: \(args[i])\n", stderr)
                exit(1)
            }
        }
        i += 1
    }

    guard let filePath else {
        fputs("Error: no input file specified\n", stderr)
        exit(1)
    }

    // Resolve TOML path: default is next to this binary's package, or relative
    let resolvedToml = tomlPath ?? defaultTomlPath()

    guard let tomlData = FileManager.default.contents(atPath: resolvedToml),
          let tomlString = String(data: tomlData, encoding: .utf8) else {
        fputs("Error: cannot read TOML file: \(resolvedToml)\n", stderr)
        exit(1)
    }

    let rules = parseToml(tomlString)

    guard let sourceData = FileManager.default.contents(atPath: filePath),
          let source = String(data: sourceData, encoding: .utf8) else {
        fputs("Error: cannot read source file: \(filePath)\n", stderr)
        exit(1)
    }

    let tree = Parser.parse(source: source)
    let rewriter = TokenSwapRewriter(rules: rules)
    let rewritten = rewriter.visit(tree)

    let result = rewritten.description
    try result.write(toFile: filePath, atomically: true, encoding: .utf8)
    fputs("apply-token-swap: rewrote \(filePath)\n", stderr)
}

func defaultTomlPath() -> String {
    // Walk up from this file's source location to find ci/migration/token-swap.toml.
    // At runtime we look relative to the CWD.
    let cwd = FileManager.default.currentDirectoryPath
    return (cwd as NSString).appendingPathComponent("ci/migration/token-swap.toml")
}

func printHelp() {
    print("""
    apply-token-swap — AST-level token-swap tool for the NMP migration.

    Usage:
      apply-token-swap <file.swift> [--toml <token-swap.toml>]
      apply-token-swap --help

    Arguments:
      <file.swift>          Swift source file to rewrite (in place).
      --toml <path>         Path to token-swap.toml.
                            Default: <cwd>/ci/migration/token-swap.toml

    Token swaps are applied at AST level using SwiftSyntax.
    Only identifier renames, import removals, and known member-call
    rewrites are performed — never raw string replacement.
    """)
}

// ── TOML parser (minimal, for our specific schema) ────────────────────────────

struct SwapRule {
    enum Kind: String { case identifier, member_call, import_remove }
    let kind: Kind
    let from: String
    let to: String
    let argTransform: String?
}

func parseToml(_ source: String) -> [SwapRule] {
    var rules: [SwapRule] = []
    var currentKind: SwapRule.Kind? = nil
    var currentFrom: String? = nil
    var currentTo: String? = nil
    var currentArgTransform: String? = nil

    func flush() {
        guard let k = currentKind, let f = currentFrom, let t = currentTo else { return }
        rules.append(SwapRule(kind: k, from: f, to: t, argTransform: currentArgTransform))
        currentKind = nil
        currentFrom = nil
        currentTo = nil
        currentArgTransform = nil
    }

    for rawLine in source.components(separatedBy: "\n") {
        let line = rawLine.trimmingCharacters(in: .whitespaces)
        if line.isEmpty || line.hasPrefix("#") { continue }

        if line == "[[swap]]" {
            flush()
            continue
        }

        let parts = line.components(separatedBy: "=")
        guard parts.count >= 2 else { continue }
        let key = parts[0].trimmingCharacters(in: .whitespaces)
        let value = parts.dropFirst().joined(separator: "=")
            .trimmingCharacters(in: .whitespaces)
            .trimmingCharacters(in: CharacterSet(charactersIn: "\""))

        switch key {
        case "kind":
            currentKind = SwapRule.Kind(rawValue: value)
        case "from":
            currentFrom = value
        case "to":
            currentTo = value
        case "arg_transform":
            currentArgTransform = value
        default:
            break
        }
    }
    flush()
    return rules
}

// ── SwiftSyntax rewriter ──────────────────────────────────────────────────────

final class TokenSwapRewriter: SyntaxRewriter {
    let rules: [SwapRule]

    init(rules: [SwapRule]) {
        self.rules = rules
    }

    // ── Import removal ─────────────────────────────────────────────────────
    // Import declarations live inside a CodeBlockItemListSyntax.  We filter
    // out items whose item is an ImportDeclSyntax matching a removal rule.

    override func visit(_ node: CodeBlockItemListSyntax) -> CodeBlockItemListSyntax {
        // First let the default rewriter descend into children.
        let visited = super.visit(node)
        // Then filter out import declarations that match removal rules.
        let filtered = visited.filter { item in
            guard let importDecl = item.item.as(ImportDeclSyntax.self) else {
                return true  // keep all non-import items
            }
            let moduleName = importDecl.path.map { $0.name.text }.joined(separator: ".")
            let shouldRemove = rules.contains {
                $0.kind == .import_remove && $0.from == moduleName
            }
            return !shouldRemove
        }
        return filtered
    }

    // ── Identifier rename ──────────────────────────────────────────────────

    override func visit(_ token: TokenSyntax) -> TokenSyntax {
        guard case .identifier(let text) = token.tokenKind else {
            return token
        }
        for rule in rules where rule.kind == .identifier && rule.from == text {
            return token.with(\.tokenKind, .identifier(rule.to))
        }
        return token
    }

    // ── Member-call rewrite ────────────────────────────────────────────────
    // Pattern: <Base>.shared.<method>(args)
    // Matches rule.from = "Base.shared.method"
    // Replaces with: model.<to-method>(args)  [or with arg_transform applied]

    override func visit(_ node: FunctionCallExprSyntax) -> ExprSyntax {
        guard let rewritten = rewriteMemberCall(node) else {
            return ExprSyntax(node)
        }
        return ExprSyntax(rewritten)
    }

    private func rewriteMemberCall(_ node: FunctionCallExprSyntax) -> FunctionCallExprSyntax? {
        // We expect: Base.shared.method(args)
        // calledExpression is a MemberAccessExprSyntax where:
        //   .base = Base.shared  (another MemberAccessExprSyntax)
        //   .declName = method
        guard let outerMember = node.calledExpression.as(MemberAccessExprSyntax.self),
              let innerMember = outerMember.base?.as(MemberAccessExprSyntax.self),
              let baseExpr = innerMember.base?.as(DeclReferenceExprSyntax.self) else {
            return nil
        }

        let baseName = baseExpr.baseName.text        // e.g. "AudioEngine"
        let sharedName = innerMember.declName.baseName.text  // e.g. "shared"
        let methodName = outerMember.declName.baseName.text  // e.g. "play"

        guard sharedName == "shared" else { return nil }

        let fromPattern = "\(baseName).shared.\(methodName)"

        for rule in rules where rule.kind == .member_call && rule.from == fromPattern {
            // Build replacement: model.<to>(args)
            let modelBase = DeclReferenceExprSyntax(
                baseName: .identifier("model")
            )
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
                // Wrap first argument in a member access: arg → arg.id
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

// ── Run ───────────────────────────────────────────────────────────────────────

do {
    try main()
} catch {
    fputs("Error: \(error)\n", stderr)
    exit(1)
}
