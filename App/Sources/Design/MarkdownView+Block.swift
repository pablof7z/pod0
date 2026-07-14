import Foundation

// MARK: - Block parser

extension MarkdownView {

    enum Block {
        case h1(String)
        case h2(String)
        case h3(String)
        case paragraph(String)
        /// A GFM task list. Each item carries its text and checked state.
        case taskList([(text: String, checked: Bool)])
        case bullets([String])
        case numberedList([String])
        case quote(String)
        /// A fenced code block (``` ... ```). `language` is the optional info string
        /// after the opening fence (e.g. "swift", "json"). `code` is the raw content.
        case codeBlock(language: String?, code: String)
        /// A GFM-style pipe table. `headers` is the column header row (may be empty
        /// strings when the header row is blank). `rows` is the data rows — each
        /// inner array has the same column count as `headers` (padded or trimmed).
        case table(headers: [String], rows: [[String]])

        static func parse(_ text: String) -> [Block] {
            var blocks: [Block] = []
            var paragraphLines: [String] = []
            var bulletLines: [String] = []
            var numberedLines: [String] = []
            var quoteLines: [String] = []
            var taskItems: [(text: String, checked: Bool)] = []

            // Code-block accumulation state.
            var inCodeBlock = false
            var codeLanguage: String? = nil
            var codeLines: [String] = []

            // Table accumulation state.
            // A GFM table consists of: a header row | separator row (---) | data rows.
            // We accumulate pipe-delimited lines; when the second line looks like a
            // separator (only |, -, :, and spaces) we commit to building a table.
            var tableHeaderCells: [String]? = nil   // set after we see a separator row
            var tableRows: [[String]] = []

            func flushParagraph() {
                guard !paragraphLines.isEmpty else { return }
                blocks.append(.paragraph(paragraphLines.joined(separator: " ")))
                paragraphLines.removeAll()
            }
            func flushBullets() {
                guard !bulletLines.isEmpty else { return }
                blocks.append(.bullets(bulletLines))
                bulletLines.removeAll()
            }
            func flushNumbered() {
                guard !numberedLines.isEmpty else { return }
                blocks.append(.numberedList(numberedLines))
                numberedLines.removeAll()
            }
            func flushQuote() {
                guard !quoteLines.isEmpty else { return }
                blocks.append(.quote(quoteLines.joined(separator: " ")))
                quoteLines.removeAll()
            }
            func flushTaskList() {
                guard !taskItems.isEmpty else { return }
                blocks.append(.taskList(taskItems))
                taskItems.removeAll()
            }
            func flushTable() {
                guard let headers = tableHeaderCells, !tableRows.isEmpty else {
                    // Incomplete table (no separator seen or no data rows): emit
                    // any collected header as a paragraph so nothing is lost.
                    if let raw = tableHeaderCells {
                        blocks.append(.paragraph(raw.joined(separator: " | ")))
                    }
                    tableHeaderCells = nil
                    tableRows.removeAll()
                    return
                }
                blocks.append(.table(headers: headers, rows: tableRows))
                tableHeaderCells = nil
                tableRows.removeAll()
            }
            func flushAll() { flushParagraph(); flushBullets(); flushNumbered(); flushQuote(); flushTable(); flushTaskList() }
            func flushCode() {
                // Emit whatever has been collected, even if the closing fence is missing.
                let code = codeLines.joined(separator: "\n")
                blocks.append(.codeBlock(language: codeLanguage, code: code))
                codeLines.removeAll()
                codeLanguage = nil
                inCodeBlock = false
            }

            // Pending header line that arrived before a separator was confirmed.
            var pendingTableHeader: [String]? = nil

            for rawLine in text.components(separatedBy: "\n") {
                // Inside a code block all lines are literal — only a closing
                // fence (``` with no other content) ends it.
                if inCodeBlock {
                    let stripped = rawLine.trimmingCharacters(in: .whitespaces)
                    if stripped == "```" || stripped == "~~~" {
                        flushCode()
                    } else {
                        codeLines.append(rawLine)
                    }
                    continue
                }

                let line = rawLine.trimmingCharacters(in: .whitespaces)

                // Opening fence: ``` or ~~~ optionally followed by a language tag.
                if line.hasPrefix("```") || line.hasPrefix("~~~") {
                    flushAll()
                    pendingTableHeader = nil
                    let fence: String = line.hasPrefix("```") ? "```" : "~~~"
                    let tag = String(line.dropFirst(fence.count))
                        .trimmingCharacters(in: .whitespaces)
                    codeLanguage = tag.isEmpty ? nil : tag
                    inCodeBlock = true
                    continue
                }

                if line.isEmpty {
                    // An empty line terminates any table in progress.
                    if pendingTableHeader != nil {
                        // Never confirmed as a table header — emit as paragraph.
                        if let cells = pendingTableHeader {
                            flushAll()
                            blocks.append(.paragraph(cells.joined(separator: " | ")))
                        }
                        pendingTableHeader = nil
                    }
                    flushAll()
                    continue
                }

                // ── Table detection ──────────────────────────────────────────────
                // A pipe-delimited line either starts/continues a table or is a
                // paragraph with pipes in it. We identify tables by requiring a
                // separator row (cells containing only -, :, and spaces, optionally
                // wrapped in pipes).
                if isTableLine(line) {
                    if let confirmedCells = tableHeaderCells {
                        // We're inside a confirmed table — add a data row.
                        tableRows.append(tableCells(from: line, columnCount: confirmedCells.count))
                    } else if let pending = pendingTableHeader {
                        // Second pipe line — check if it's a separator row.
                        if isTableSeparatorLine(line) {
                            // Confirmed: start a table with the pending header.
                            tableHeaderCells = pending
                            pendingTableHeader = nil
                        } else {
                            // Two consecutive data-ish pipe lines without a separator:
                            // emit the first as a paragraph, treat the current as a new candidate.
                            flushAll()
                            blocks.append(.paragraph(pending.joined(separator: " | ")))
                            pendingTableHeader = tableCells(from: line, columnCount: nil)
                        }
                    } else {
                        // First pipe line — hold as pending header.
                        flushAll()
                        pendingTableHeader = tableCells(from: line, columnCount: nil)
                    }
                    continue
                }

                // Not a pipe line — flush any pending table state.
                if let pending = pendingTableHeader {
                    blocks.append(.paragraph(pending.joined(separator: " | ")))
                    pendingTableHeader = nil
                }
                if tableHeaderCells != nil {
                    flushTable()
                }
                // ── End table detection ──────────────────────────────────────────

                if line.hasPrefix("### ") {
                    flushAll(); blocks.append(.h3(String(line.dropFirst(4))))
                } else if line.hasPrefix("## ") {
                    flushAll(); blocks.append(.h2(String(line.dropFirst(3))))
                } else if line.hasPrefix("# ") {
                    flushAll(); blocks.append(.h1(String(line.dropFirst(2))))
                } else if let taskItem = parseTaskListItem(line) {
                    // GFM task list: `- [ ] text` or `- [x] text` (case-insensitive).
                    // Must be checked before the plain bullet branch.
                    flushParagraph(); flushQuote(); flushNumbered(); flushBullets()
                    taskItems.append(taskItem)
                } else if line.hasPrefix("- ") || line.hasPrefix("* ") {
                    flushParagraph(); flushQuote(); flushNumbered(); flushTaskList()
                    bulletLines.append(String(line.dropFirst(2)))
                } else if isNumberedItem(line) {
                    flushParagraph(); flushQuote(); flushBullets(); flushTaskList()
                    numberedLines.append(stripNumberedPrefix(line))
                } else if line.hasPrefix("> ") {
                    flushParagraph(); flushBullets(); flushNumbered(); flushTaskList()
                    quoteLines.append(String(line.dropFirst(2)))
                } else {
                    flushBullets(); flushNumbered(); flushQuote(); flushTaskList()
                    paragraphLines.append(line)
                }
            }

            // Flush any pending table header that was never confirmed.
            if let pending = pendingTableHeader {
                blocks.append(.paragraph(pending.joined(separator: " | ")))
            }
            // Gracefully close any unclosed fenced code block.
            if inCodeBlock { flushCode() }
            flushAll()
            return blocks
        }

        // MARK: - Table helpers

        /// Returns true when `line` looks like a GFM table row — contains at
        /// least one `|` character (leading/trailing pipes are optional).
        private static func isTableLine(_ line: String) -> Bool {
            line.contains("|")
        }

        /// Returns true when every non-pipe, non-whitespace character in `line`
        /// is a `-` or `:` — the GFM separator row pattern.
        private static func isTableSeparatorLine(_ line: String) -> Bool {
            guard line.contains("|") else { return false }
            let stripped = line.replacingOccurrences(of: "|", with: "")
                .replacingOccurrences(of: " ", with: "")
                .replacingOccurrences(of: ":", with: "")
                .replacingOccurrences(of: "-", with: "")
            return stripped.isEmpty
        }

        /// Splits a pipe-delimited line into trimmed cell strings.
        /// Leading/trailing pipes are stripped. When `columnCount` is non-nil the
        /// result is padded with empty strings or truncated to match.
        private static func tableCells(from line: String, columnCount: Int?) -> [String] {
            var stripped = line
            if stripped.hasPrefix("|") { stripped = String(stripped.dropFirst()) }
            if stripped.hasSuffix("|") { stripped = String(stripped.dropLast()) }
            var cells = stripped.components(separatedBy: "|")
                .map { $0.trimmingCharacters(in: .whitespaces) }
            if let count = columnCount {
                while cells.count < count { cells.append("") }
                cells = Array(cells.prefix(count))
            }
            return cells
        }

        private static func isNumberedItem(_ line: String) -> Bool {
            guard let first = line.first, first.isNumber else { return false }
            let afterDigits = line.drop(while: { $0.isNumber })
            return afterDigits.hasPrefix(". ")
        }

        private static func stripNumberedPrefix(_ line: String) -> String {
            let afterDigits = line.drop(while: { $0.isNumber })
            guard afterDigits.hasPrefix(". ") else { return line }
            return String(afterDigits.dropFirst(2))
        }

        /// Parses a GFM task list item.
        ///
        /// Accepts `- [ ] text`, `- [x] text`, `- [X] text`, `* [ ] text`, etc.
        /// Returns `nil` when the line is a plain bullet without a checkbox.
        private static func parseTaskListItem(_ line: String) -> (text: String, checked: Bool)? {
            var rest: Substring
            if line.hasPrefix("- ") {
                rest = line.dropFirst(2)
            } else if line.hasPrefix("* ") {
                rest = line.dropFirst(2)
            } else {
                return nil
            }
            if rest.hasPrefix("[ ] ") {
                return (text: String(rest.dropFirst(4)), checked: false)
            } else if rest.hasPrefix("[x] ") || rest.hasPrefix("[X] ") {
                return (text: String(rest.dropFirst(4)), checked: true)
            }
            return nil
        }
    }
}
