import SwiftUI

// MARK: - ScreenshotAnnotationView

struct ScreenshotAnnotationView: View {

    private struct Stroke: Equatable {
        var points: [CGPoint]
        var color: Color
        var width: CGFloat
    }

    @Bindable var workflow: FeedbackWorkflow
    @Environment(\.dismiss) private var dismiss

    @State private var strokes: [Stroke] = []
    @State private var currentStroke: Stroke?
    @State private var strokeColor: Color = .red
    @State private var strokeWidth: CGFloat = Layout.defaultStrokeWidth
    @State private var canvasWidth: CGFloat = 0

    private let palette: [Color] = [.red, .orange, .blue, .yellow, .white]

    private enum Layout {
        /// Default pen width when the view opens.
        static let defaultStrokeWidth: CGFloat = 3.0
        /// Stroke width slider range.
        static let strokeWidthRange: ClosedRange<CGFloat> = 1.5...8.0
        /// Diameter of each color-swatch circle in the toolbar.
        static let swatchSize: CGFloat = 26
        /// Line width of the selection ring drawn around the active swatch.
        static let swatchSelectionRing: CGFloat = 2
        /// Scale factor applied to the selected swatch.
        static let swatchSelectedScale: CGFloat = 1.18
        /// Fixed width of the width-picker slider.
        static let sliderWidth: CGFloat = 72
        /// Horizontal spacing between toolbar items.
        static let toolbarSpacing: CGFloat = 12
        /// Horizontal padding inside the toolbar pill.
        static let toolbarPaddingH: CGFloat = 16
        /// Vertical padding inside the toolbar pill.
        static let toolbarPaddingV: CGFloat = 10
        /// Extra bottom padding below the toolbar pill.
        static let toolbarPaddingBottom: CGFloat = 8
        /// Width of the thin separator lines in the toolbar.
        static let separatorWidth: CGFloat = 1
        /// Height of the thin separator lines in the toolbar.
        static let separatorHeight: CGFloat = 24
        /// Horizontal gap between the pencil-tip icon and the slider.
        static let sliderIconSpacing: CGFloat = 6
    }

    var body: some View {
        NavigationStack {
            ZStack {
                Color.black.ignoresSafeArea()

                if let screenshot = workflow.screenshot {
                    Image(uiImage: screenshot)
                        .resizable()
                        .scaledToFit()
                }

                Canvas { context, _ in
                    for stroke in strokes {
                        drawStroke(stroke, in: &context)
                    }
                    if let current = currentStroke {
                        drawStroke(current, in: &context)
                    }
                }
                .gesture(
                    DragGesture(minimumDistance: 0)
                        .onChanged { value in
                            if currentStroke == nil {
                                currentStroke = Stroke(
                                    points: [value.location],
                                    color: strokeColor,
                                    width: strokeWidth
                                )
                            } else {
                                currentStroke?.points.append(value.location)
                            }
                        }
                        .onEnded { _ in
                            if let stroke = currentStroke, stroke.points.count > 1 {
                                strokes.append(stroke)
                                Haptics.light()
                            }
                            currentStroke = nil
                        }
                )
            }
            .onGeometryChange(for: CGFloat.self) { proxy in
                proxy.size.width
            } action: { newWidth in
                canvasWidth = newWidth
            }
            .navigationTitle("Annotate")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel") {
                        workflow.phase = .composing
                        dismiss()
                    }
                }
                ToolbarItem(placement: .confirmationAction) {
                    Button("Done") { saveAnnotation() }
                        .fontWeight(.semibold)
                }
            }
            .safeAreaInset(edge: .bottom) {
                drawingToolbar
            }
        }
    }

    // MARK: - Drawing toolbar

    @ViewBuilder
    private var drawingToolbar: some View {
        GlassEffectContainer(spacing: Layout.toolbarSpacing) {
            HStack(spacing: Layout.toolbarSpacing) {
                // Color palette
                ForEach(palette, id: \.self) { color in
                    Button {
                        strokeColor = color
                        Haptics.selection()
                    } label: {
                        ZStack {
                            Circle()
                                .fill(color)
                                .frame(width: Layout.swatchSize, height: Layout.swatchSize)
                            if strokeColor == color {
                                Circle()
                                    .strokeBorder(.white, lineWidth: Layout.swatchSelectionRing)
                                    .frame(width: Layout.swatchSize, height: Layout.swatchSize)
                            }
                        }
                    }
                    .buttonStyle(.plain)
                    .scaleEffect(strokeColor == color ? Layout.swatchSelectedScale : 1.0)
                    .animation(AppTheme.Animation.springFast, value: strokeColor == color)
                    .accessibilityLabel(
                        strokeColor == color
                            ? "Drawing color selected"
                            : "Select drawing color"
                    )
                }

                Rectangle()
                    .fill(.separator)
                    .frame(width: Layout.separatorWidth, height: Layout.separatorHeight)

                // Width slider
                HStack(spacing: Layout.sliderIconSpacing) {
                    Image(systemName: "pencil.tip")
                        .font(AppTheme.Typography.caption2)
                        .foregroundStyle(.secondary)
                    Slider(value: $strokeWidth, in: Layout.strokeWidthRange)
                        .frame(width: Layout.sliderWidth)
                        .tint(strokeColor)
                }

                Rectangle()
                    .fill(.separator)
                    .frame(width: Layout.separatorWidth, height: Layout.separatorHeight)

                // Undo
                Button {
                    guard !strokes.isEmpty else { return }
                    strokes.removeLast()
                    Haptics.selection()
                } label: {
                    Image(systemName: "arrow.uturn.backward")
                        .font(AppTheme.Typography.callout)
                }
                .buttonStyle(.glass)
                .disabled(strokes.isEmpty)
                .accessibilityLabel("Undo last stroke")

                // Clear
                Button {
                    strokes = []
                    currentStroke = nil
                    Haptics.medium()
                } label: {
                    Image(systemName: "trash")
                        .font(AppTheme.Typography.callout)
                }
                .buttonStyle(.glass)
                .disabled(strokes.isEmpty && currentStroke == nil)
                .accessibilityLabel("Clear all drawing")
            }
            .padding(.horizontal, Layout.toolbarPaddingH)
            .padding(.vertical, Layout.toolbarPaddingV)
            .padding(.bottom, Layout.toolbarPaddingBottom)
        }
    }

    // MARK: - Drawing

    private func drawStroke(_ stroke: Stroke, in context: inout GraphicsContext) {
        guard stroke.points.count > 1 else { return }
        var path = Path()
        path.move(to: stroke.points[0])
        for point in stroke.points.dropFirst() {
            path.addLine(to: point)
        }
        context.stroke(
            path,
            with: .color(stroke.color),
            style: StrokeStyle(lineWidth: stroke.width, lineCap: .round, lineJoin: .round)
        )
    }

    // MARK: - Save

    private func saveAnnotation() {
        guard let screenshot = workflow.screenshot else {
            workflow.phase = .composing
            dismiss()
            return
        }

        let format = UIGraphicsImageRendererFormat()
        format.scale = screenshot.scale
        let renderer = UIGraphicsImageRenderer(size: screenshot.size, format: format)
        let annotated = renderer.image { ctx in
            screenshot.draw(at: .zero)
            let scale = canvasWidth > 0 ? screenshot.size.width / canvasWidth : 1
            ctx.cgContext.scaleBy(x: scale, y: scale)

            for stroke in strokes {
                guard stroke.points.count > 1 else { continue }
                let uiColor = UIColor(stroke.color)
                ctx.cgContext.setStrokeColor(uiColor.cgColor)
                ctx.cgContext.setLineWidth(stroke.width)
                ctx.cgContext.setLineCap(.round)
                ctx.cgContext.setLineJoin(.round)
                ctx.cgContext.move(to: stroke.points[0])
                for point in stroke.points.dropFirst() {
                    ctx.cgContext.addLine(to: point)
                }
                ctx.cgContext.strokePath()
            }
        }

        workflow.annotatedImage = annotated
        workflow.phase = .composing
        dismiss()
    }
}
