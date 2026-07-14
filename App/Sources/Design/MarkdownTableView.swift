import SwiftUI

// MARK: - MarkdownTableView

/// Renders a GFM-style pipe table with a styled header row, alternating row
/// backgrounds, and horizontal scrolling for wide tables.
///
/// Column widths are determined by the widest content in each column so that
/// text doesn't wrap inside cells, keeping the table scannable at a glance.
struct MarkdownTableView: View {

    let headers: [String]
    let rows: [[String]]

    private enum Layout {
        /// Horizontal padding inside each table cell.
        static let cellPaddingH: CGFloat = AppTheme.Spacing.sm
        /// Vertical padding inside each table cell.
        static let cellPaddingV: CGFloat = AppTheme.Spacing.xs
        /// Minimum column width so single-character columns aren't too narrow.
        static let minColumnWidth: CGFloat = 40
    }

    var body: some View {
        ScrollView(.horizontal, showsIndicators: false) {
            VStack(alignment: .leading, spacing: 0) {
                // Header row
                if headers.contains(where: { !$0.isEmpty }) {
                    headerRow
                    Divider()
                        .opacity(0.4)
                }
                // Data rows
                ForEach(Array(rows.enumerated()), id: \.offset) { index, row in
                    dataRow(row, isEven: index.isMultiple(of: 2))
                    if index < rows.count - 1 {
                        Divider()
                            .opacity(0.2)
                    }
                }
            }
            .background(
                Color(.secondarySystemBackground).opacity(0.6),
                in: RoundedRectangle(cornerRadius: AppTheme.Corner.md, style: .continuous)
            )
            .overlay(
                RoundedRectangle(cornerRadius: AppTheme.Corner.md, style: .continuous)
                    .strokeBorder(Color.secondary.opacity(0.18), lineWidth: 0.5)
            )
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    // MARK: - Rows

    private var headerRow: some View {
        HStack(alignment: .center, spacing: 0) {
            ForEach(Array(headers.enumerated()), id: \.offset) { index, header in
                Text(header)
                    .font(AppTheme.Typography.caption.weight(.semibold))
                    .foregroundStyle(.primary)
                    .lineLimit(1)
                    .padding(.horizontal, Layout.cellPaddingH)
                    .padding(.vertical, Layout.cellPaddingV)
                    .frame(minWidth: Layout.minColumnWidth, alignment: .leading)
                if index < headers.count - 1 {
                    Divider()
                        .opacity(0.3)
                }
            }
        }
        .background(Color.secondary.opacity(0.08))
        .accessibilityElement(children: .combine)
        .accessibilityLabel(headers.joined(separator: ", "))
    }

    private func dataRow(_ cells: [String], isEven: Bool) -> some View {
        HStack(alignment: .top, spacing: 0) {
            ForEach(Array(cells.enumerated()), id: \.offset) { index, cell in
                Text(cell)
                    .font(AppTheme.Typography.callout)
                    .foregroundStyle(.primary)
                    .lineLimit(3)
                    .padding(.horizontal, Layout.cellPaddingH)
                    .padding(.vertical, Layout.cellPaddingV)
                    .frame(minWidth: Layout.minColumnWidth, alignment: .leading)
                if index < cells.count - 1 {
                    Divider()
                        .opacity(0.2)
                }
            }
        }
        .background(isEven ? Color.clear : Color.secondary.opacity(0.04))
        .accessibilityElement(children: .combine)
        .accessibilityLabel(cells.joined(separator: ", "))
    }
}
