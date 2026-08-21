import AppKit
import SwiftUI

// MARK: - ViewModel

enum FloatingTranscriptPhase: Equatable {
    case preparing
    case ready
    case listening
    case finishing
}

private enum MluvaColors {
    static let ready = Color(nsColor: .systemGreen)
    static let recording = Color(nsColor: .systemRed)
    static let preparing = Color.accentColor
}

final class FloatingTranscriptViewModel: ObservableObject {
    @Published var text: String = ""
    @Published var cleanupSegments: [CleanupSegmentProjection] = []
    @Published var phase: FloatingTranscriptPhase = .preparing
    @Published var audioLevel = 0.0
    @Published var providerName = ""
    @Published var cleanupProviderName = ""
    @Published var cleanupModelIdentifier = ""
    @Published var disclosedContext = ""
    @Published var inputDeviceName = ""
    @Published var startedAt: Date?

    var isRecording: Bool {
        phase == .ready || phase == .listening
    }
}

// MARK: - SwiftUI View

struct FloatingTranscriptContent: View {
    @ObservedObject var viewModel: FloatingTranscriptViewModel

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack(spacing: 8) {
                phaseIndicator

                if viewModel.text.isEmpty && viewModel.cleanupSegments.isEmpty {
                    VStack(alignment: .leading, spacing: 1) {
                        Text(primaryStatus)
                            .font(.system(size: 13, weight: viewModel.phase == .ready ? .semibold : .regular))
                            .foregroundStyle(viewModel.phase == .ready ? MluvaColors.ready : .primary)
                        if viewModel.phase == .preparing {
                            Text("Wait for Ready before speaking")
                                .font(.caption2)
                                .foregroundStyle(.secondary)
                        }
                    }
                } else if !viewModel.text.isEmpty {
                    Text(viewModel.text)
                        .font(.system(size: 13))
                        .foregroundColor(.primary)
                        .lineLimit(3)
                }

                Spacer(minLength: 0)
            }

            ForEach(viewModel.cleanupSegments.suffix(3)) { segment in
                VStack(alignment: .leading, spacing: 2) {
                    Text("\(segment.state.displayName) · Raw: \(segment.rawText)")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                        .lineLimit(2)
                    if let revisedText = segment.revisedText {
                        Text("Cleaned: \(revisedText)")
                            .font(.system(size: 13))
                            .foregroundStyle(.primary)
                            .lineLimit(2)
                    }
                }
                .accessibilityElement(children: .combine)
                .accessibilityLabel(segment.accessibilityLabel)
            }

            if viewModel.phase != .preparing || !viewModel.providerName.isEmpty {
                HStack(spacing: 5) {
                    Text(viewModel.providerName)
                    if !viewModel.cleanupProviderName.isEmpty {
                        Text("·")
                        Text("\(viewModel.cleanupProviderName) · \(viewModel.cleanupModelIdentifier)")
                            .lineLimit(1)
                    }
                    if !viewModel.disclosedContext.isEmpty {
                        Text("·")
                        Text("Context: \(viewModel.disclosedContext)")
                            .lineLimit(1)
                    }
                    if !viewModel.inputDeviceName.isEmpty {
                        Text("·")
                        Text(viewModel.inputDeviceName)
                            .lineLimit(1)
                    }
                    Spacer(minLength: 2)
                    if let startedAt = viewModel.startedAt {
                        ElapsedRecordingTime(startedAt: startedAt)
                    }
                }
                .font(.caption2)
                .foregroundStyle(.secondary)
            }
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 10)
        .background(
            RoundedRectangle(cornerRadius: 10)
                .fill(.ultraThinMaterial)
                .shadow(color: .black.opacity(0.15), radius: 8, y: 2)
        )
        .animation(.easeInOut(duration: 0.2), value: viewModel.phase)
        .accessibilityElement(children: .combine)
        .accessibilityLabel(primaryStatus)
    }

    @ViewBuilder
    private var phaseIndicator: some View {
        switch viewModel.phase {
        case .preparing, .finishing:
            ProgressView()
                .controlSize(.small)
                .tint(MluvaColors.preparing)
                .frame(width: 23, height: 20)
        case .ready:
            Image(systemName: "checkmark.circle.fill")
                .font(.system(size: 19, weight: .semibold))
                .foregroundStyle(MluvaColors.ready)
                .frame(width: 23, height: 20)
                .transition(.scale.combined(with: .opacity))
        case .listening:
            AudioLevelBars(level: viewModel.audioLevel)
        }
    }

    private var primaryStatus: String {
        switch viewModel.phase {
        case .preparing:
            viewModel.providerName.isEmpty
                ? "Preparing dictation…"
                : "Connecting to \(viewModel.providerName)…"
        case .ready:
            "Ready — speak now"
        case .listening:
            "Listening…"
        case .finishing:
            "Finishing…"
        }
    }
}

// MARK: - NSPanel Window

final class FloatingTranscriptWindow: NSPanel {
    static let shared = FloatingTranscriptWindow()

    private let hostingView: NSHostingView<FloatingTranscriptContent>
    private let viewModel = FloatingTranscriptViewModel()

    private init() {
        hostingView = NSHostingView(rootView: FloatingTranscriptContent(viewModel: viewModel))

        super.init(
            contentRect: NSRect(x: 0, y: 0, width: 300, height: 44),
            styleMask: [.borderless, .nonactivatingPanel],
            backing: .buffered,
            defer: false
        )

        isOpaque = false
        backgroundColor = .clear
        level = .floating
        collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary]
        isMovableByWindowBackground = true
        hidesOnDeactivate = false

        contentView = hostingView

        // Position at bottom center of main screen
        positionAtBottomCenter()
    }

    override var canBecomeKey: Bool { false }
    override var canBecomeMain: Bool { false }

    func showPreparing(provider: TranscriptionProviderKind) {
        viewModel.phase = .preparing
        viewModel.text = ""
        viewModel.cleanupSegments = []
        viewModel.audioLevel = 0
        viewModel.providerName = provider == .automatic ? "" : provider.displayName
        viewModel.cleanupProviderName = ""
        viewModel.cleanupModelIdentifier = ""
        viewModel.disclosedContext = ""
        viewModel.inputDeviceName = ""
        viewModel.startedAt = nil
        present()
    }

    func updatePreparing(provider: TranscriptionProviderKind) {
        viewModel.phase = .preparing
        viewModel.providerName = provider.displayName
    }

    func showReady(
        provider: TranscriptionProviderKind,
        cleanupProviderName: String = "",
        cleanupModelIdentifier: String = "",
        disclosedContextSources: [TranscriptContextSource] = [],
        inputDeviceName: String?,
        startedAt: Date?
    ) {
        viewModel.phase = .ready
        viewModel.text = ""
        viewModel.cleanupSegments = []
        viewModel.audioLevel = 0
        viewModel.providerName = provider.displayName
        viewModel.cleanupProviderName = cleanupProviderName
        viewModel.cleanupModelIdentifier = cleanupModelIdentifier
        viewModel.disclosedContext = disclosedContextSources
            .map(\.displayName)
            .joined(separator: ", ")
        viewModel.inputDeviceName = inputDeviceName ?? ""
        viewModel.startedAt = startedAt
        present()

        Task { @MainActor [weak self] in
            try? await Task.sleep(for: .milliseconds(850))
            guard let self, self.viewModel.phase == .ready else { return }
            self.viewModel.phase = .listening
        }
    }

    func showFinishing() {
        viewModel.phase = .finishing
        viewModel.text = ""
        resizeForContent()
    }

    func updateText(_ text: String) {
        if viewModel.phase == .ready {
            viewModel.phase = .listening
        }
        viewModel.text = text
        resizeForContent()
    }

    func updateCleanupProjection(_ projection: CleanupSessionProjection) {
        viewModel.text = projection.volatileText ?? ""
        viewModel.cleanupSegments = projection.segments
        viewModel.cleanupProviderName = projection.cleanupProviderName
        viewModel.cleanupModelIdentifier = projection.cleanupModelIdentifier
        viewModel.disclosedContext = projection.disclosedContextSources
            .map(\.displayName)
            .joined(separator: ", ")
        resizeForContent()
    }

    private func resizeForContent() {
        let text = viewModel.text + viewModel.cleanupSegments.map {
            $0.rawText + ($0.revisedText ?? "")
        }.joined()

        // Resize to fit content
        let baseWidth: CGFloat = 300
        let textWidth = CGFloat(text.count) * 7.5 + 80
        let width = max(200, min(500, max(baseWidth, textWidth)))

        let lines = max(1, (text.count / 50) + 1)
        let segmentLines = min(viewModel.cleanupSegments.count, 3) * 38
        let height = CGFloat(min(lines, 3)) * 18 + 34 + CGFloat(segmentLines)

        setContentSize(NSSize(width: width, height: height))

        // Re-center horizontally
        if let screen = NSScreen.main {
            let screenFrame = screen.visibleFrame
            let x = screenFrame.midX - frame.width / 2
            setFrameOrigin(NSPoint(x: x, y: frame.origin.y))
        }
    }

    func updateProvider(_ provider: TranscriptionProviderKind) {
        viewModel.providerName = provider.displayName
    }

    func updateAudioLevel(_ level: Double) {
        if viewModel.phase == .ready, level > 0.02 {
            viewModel.phase = .listening
        }
        viewModel.audioLevel = level
    }

    func hide() {
        NSAnimationContext.runAnimationGroup({ context in
            context.duration = 0.2
            self.animator().alphaValue = 0
        }) {
            self.orderOut(nil)
            self.viewModel.text = ""
            self.viewModel.cleanupSegments = []
            self.viewModel.phase = .preparing
            self.viewModel.audioLevel = 0
            self.viewModel.startedAt = nil
        }
    }

    private func present() {
        positionAtBottomCenter()
        resizeForContent()
        guard !isVisible else { return }
        alphaValue = 0
        orderFront(nil)
        NSAnimationContext.runAnimationGroup { context in
            context.duration = 0.2
            self.animator().alphaValue = 1
        }
    }

    private func positionAtBottomCenter() {
        guard let screen = NSScreen.main else { return }
        let screenFrame = screen.visibleFrame
        let x = screenFrame.midX - frame.width / 2
        let y = screenFrame.minY + 80
        setFrameOrigin(NSPoint(x: x, y: y))
    }
}

private extension CleanupSegmentProjectionState {
    var displayName: String {
        switch self {
        case .raw: "Raw"
        case .rewriting: "Rewriting"
        case .waiting: "Waiting"
        case .cleaned: "Ready"
        case .fallback: "Raw fallback"
        case .cancelled: "Cancelled"
        }
    }
}

private extension CleanupSegmentProjection {
    var accessibilityLabel: String {
        let cleaned = revisedText.map { ", cleaned text: \($0)" } ?? ""
        return "\(state.displayName), raw text: \(rawText)\(cleaned)"
    }
}

struct AudioLevelBars: View {
    let level: Double

    var body: some View {
        HStack(alignment: .center, spacing: 2) {
            ForEach(Array([0.35, 0.7, 1.0, 0.7, 0.35].enumerated()), id: \.offset) { _, scale in
                Capsule()
                    .fill(MluvaColors.recording)
                    .frame(width: 3, height: max(4, 18 * scale * max(0.15, level)))
            }
        }
        .frame(width: 23, height: 20)
        .animation(.easeOut(duration: 0.08), value: level)
    }
}

struct ElapsedRecordingTime: View {
    let startedAt: Date

    var body: some View {
        TimelineView(.periodic(from: .now, by: 1)) { context in
            Text(Self.formatted(context.date.timeIntervalSince(startedAt)))
                .monospacedDigit()
        }
    }

    private static func formatted(_ duration: TimeInterval) -> String {
        let seconds = max(0, Int(duration))
        return String(format: "%d:%02d", seconds / 60, seconds % 60)
    }
}
