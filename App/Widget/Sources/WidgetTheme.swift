import SwiftUI

// MARK: - Widget design tokens

enum WidgetTheme {

    enum Layout {
        static let maxPriorityItems = 3
        static let maxNextItems = 5
        static let mediumMaxRows = 4
        static let accessoryMaxRows = 2
        static let headerIconSpacing: CGFloat = 4
        static let itemCircleSize: CGFloat = 12
        static let itemCircleStrokeWidth: CGFloat = 1.5
        static let smallVSpacing: CGFloat = 4
    }

    enum Spacing {
        static let pad: CGFloat = 16
        static let headerTop: CGFloat = 10
        static let headerBottom: CGFloat = 6
        static let rowVertical: CGFloat = 5
        static let rowIconGap: CGFloat = 8
        static let emptyStateSpacing: CGFloat = 6
        static let accessoryRowSpacing: CGFloat = 2
    }

    enum Typography {
        static let smallIcon = Font.system(size: 20, weight: .semibold)
        static let smallCount = Font.system(size: 44, weight: .bold, design: .rounded)
        static let smallSubtitle = Font.system(size: 13, weight: .medium)
        static let header = Font.system(size: 12, weight: .semibold)
        static let itemTitle = Font.system(size: 13, weight: .regular)
        static let starIcon = Font.system(size: 9)
        static let emptyIcon = Font.system(size: 28)
        static let emptyTitle = Font.system(size: 14, weight: .semibold)
        static let emptySubtitle = Font.system(size: 12)
        static let accessoryCount = Font.system(size: 20, weight: .bold, design: .rounded)
        static let accessoryIcon = Font.system(size: 10)
        static let accessoryLabel = Font.system(size: 12)
        static let accessoryRow = Font.system(size: 12)
    }

    enum Colors {
        static let brandIndigo = Color.indigo
        static let itemCircleStroke = Color.secondary.opacity(0.4)
        static let brandGradient = LinearGradient(
            colors: [Color.indigo, Color.purple],
            startPoint: .topLeading,
            endPoint: .bottomTrailing
        )
    }
}
