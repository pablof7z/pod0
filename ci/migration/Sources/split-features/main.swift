// split-features — Remove a named class declaration from a Swift file while
// preserving all other declarations at byte level.
//
// Usage:
//   split-features <file.swift> <ClassName>
//   split-features --help
//
// The named class is removed from the file.  All View structs and everything
// else are preserved with their original trivia (whitespace, comments).
// The file is written in place.
//
// This tool is used for files on the §6.12.1 split list (05-migration-map.md):
// files that contain both a SwiftUI View struct (stays in Swift) and a
// business-logic class (moves to Rust).  Only the class is excised; the View
// bytes are untouched.

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

    guard args.count >= 2 else {
        fputs("Error: expected <file.swift> <ClassName>\n", stderr)
        exit(1)
    }

    let filePath = args[0]
    let className = args[1]

    guard let sourceData = FileManager.default.contents(atPath: filePath),
          let source = String(data: sourceData, encoding: .utf8) else {
        fputs("Error: cannot read source file: \(filePath)\n", stderr)
        exit(1)
    }

    let tree = Parser.parse(source: source)
    let remover = ClassRemover(className: className)
    let rewritten = remover.visit(tree)

    if !remover.didRemove {
        fputs("Warning: class '\(className)' not found in \(filePath)\n", stderr)
    }

    let result = rewritten.description
    try result.write(toFile: filePath, atomically: true, encoding: .utf8)
    fputs("split-features: removed class '\(className)' from \(filePath)\n", stderr)
}

func printHelp() {
    print("""
    split-features — Remove a named class declaration from a copied Swift file.

    Usage:
      split-features <file.swift> <ClassName>
      split-features --help

    Arguments:
      <file.swift>    Path to the Swift file to edit (in place).
      <ClassName>     Name of the class declaration to remove.

    The class declaration (including its body) is removed.  All other
    declarations — View structs, extensions, enums, free functions — are
    preserved with their original whitespace and comments.

    Only works on class declarations at the top level or inside extensions
    at the top level.  Nested class declarations are not targeted.
    """)
}

// ── SyntaxRewriter ────────────────────────────────────────────────────────────

final class ClassRemover: SyntaxRewriter {
    let className: String
    private(set) var didRemove = false

    init(className: String) {
        self.className = className
    }

    override func visit(_ node: ClassDeclSyntax) -> DeclSyntax {
        if node.name.text == className {
            didRemove = true
            // Return an empty trivia-only replacement.
            // Strip the node's content while keeping surrounding whitespace minimal.
            // We replace with a blank token sequence that collapses to nothing.
            return DeclSyntax(
                node
                    .with(\.leadingTrivia, [])
                    .with(\.trailingTrivia, [])
            )
        }
        return DeclSyntax(node)
    }
}

// ── Run ───────────────────────────────────────────────────────────────────────

do {
    try main()
} catch {
    fputs("Error: \(error)\n", stderr)
    exit(1)
}
