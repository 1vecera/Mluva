import AppKit
import MluvaKit
import SwiftUI

// Mluva's drafting language on the Mac: words set in JetBrains Mono like measurements, keys
// drawn as outlined legends, hairline rules instead of boxes, graphite glass for everything
// that floats, and red only for the live signal, a selection and the marks you make.

extension Font {
    /// JetBrains Mono, bundled with the app exactly as on Linux.
    static func mono(_ size: CGFloat, _ weight: Font.Weight = .regular) -> Font {
        let face = switch weight {
        case .bold, .heavy, .black, .semibold: "JetBrainsMono-Bold"
        case .medium: "JetBrainsMono-Medium"
        default: "JetBrainsMono-Regular"
        }
        return .custom(face, fixedSize: size)
    }
}

/// The few inks the drafting language uses. Floating layers are always graphite, so their
/// inks are fixed; windows follow the system appearance through `.primary` and `.secondary`.
enum Ink {
    /// Text on graphite glass.
    static let chalk = Color(white: 0.94)
    static let chalkDim = Color(white: 0.94).opacity(0.62)
    static let chalkFaint = Color(white: 0.94).opacity(0.38)
    /// A ruled line on graphite glass.
    static let rule = Color.white.opacity(0.16)
    static let graphite = Color(red: 0.07, green: 0.075, blue: 0.085)
    /// Needs attention: the only colour besides red, and only for that state.
    static let amber = Color(red: 1.0, green: 0.72, blue: 0.28)
}

extension View {
    /// The floating layer's material: Liquid Glass darkened to graphite, edged by a hairline.
    func graphiteGlass<S: InsettableShape>(_ shape: S) -> some View {
        self
            .background(Ink.graphite.opacity(0.55), in: shape)
            .glassEffect(.regular, in: shape)
            .overlay(shape.strokeBorder(Ink.rule, lineWidth: 0.75))
            .environment(\.colorScheme, .dark)
    }
}

/// A key drawn as its legend in an outlined box, the way every shortcut is shown.
struct Keycap: View {
    let label: String
    var lit = false
    var size: CGFloat = 11

    init(_ label: String, lit: Bool = false, size: CGFloat = 11) {
        self.label = label
        self.lit = lit
        self.size = size
    }

    var body: some View {
        Text(label.lowercased() == "esc" ? "esc" : label)
            .font(.mono(size, .medium))
            .padding(.horizontal, size * 0.45)
            .frame(minWidth: size * 2.1, minHeight: size * 1.9)
            .foregroundStyle(lit ? Color.white : Color.primary)
            .background(lit ? Color.mluva : .clear, in: .rect(cornerRadius: 3))
            .overlay(RoundedRectangle(cornerRadius: 3).strokeBorder(lit ? Color.mluva : Color.primary.opacity(0.45), lineWidth: 1))
            .accessibilityLabel("key \(label)")
    }
}

/// A ruled hairline, horizontal or vertical.
struct Rule: View {
    var vertical = false
    var color: Color = Color.primary.opacity(0.16)

    var body: some View {
        Rectangle()
            .fill(color)
            .frame(width: vertical ? 0.75 : nil, height: vertical ? nil : 0.75)
    }
}

/// An agent's state as a drawn mark on a fixed ramp, readable without colour: ready is a
/// solid dot, working a half-filled ring, blocked an amber diamond, unknown an empty ring and
/// draft-only an empty square.
struct StatusMark: View {
    let status: AgentStatus
    var draftOnly = false
    var size: CGFloat = 8

    var body: some View {
        Canvas { context, area in
            let rect = CGRect(origin: .zero, size: area).insetBy(dx: 0.75, dy: 0.75)
            let ink = GraphicsContext.Shading.color(.primary)
            if draftOnly {
                context.stroke(Path(rect), with: ink, lineWidth: 1)
                return
            }
            switch status {
            case .idle, .done:
                context.fill(Path(ellipseIn: rect), with: ink)
            case .working:
                context.stroke(Path(ellipseIn: rect), with: ink, lineWidth: 1)
                var half = Path()
                half.addArc(center: CGPoint(x: rect.midX, y: rect.midY), radius: rect.width / 2,
                            startAngle: .degrees(-90), endAngle: .degrees(90), clockwise: false)
                half.closeSubpath()
                context.fill(half, with: ink)
            case .blocked:
                var diamond = Path()
                diamond.move(to: CGPoint(x: rect.midX, y: rect.minY))
                diamond.addLine(to: CGPoint(x: rect.maxX, y: rect.midY))
                diamond.addLine(to: CGPoint(x: rect.midX, y: rect.maxY))
                diamond.addLine(to: CGPoint(x: rect.minX, y: rect.midY))
                diamond.closeSubpath()
                context.fill(diamond, with: .color(Ink.amber))
            case .unknown:
                context.stroke(Path(ellipseIn: rect), with: .color(.secondary), lineWidth: 1)
            }
        }
        .frame(width: size, height: size)
        .accessibilityHidden(true)
    }
}

extension AgentStatus {
    var word: String {
        switch self {
        case .idle: "ready"
        case .working: "working"
        case .blocked: "needs you"
        case .done: "turn done"
        case .unknown: "unknown"
        }
    }
}

/// The recent voice level as a hairline trace that scrolls left, newest at the right edge.
struct LevelTrace: View {
    let level: Float
    var color: Color = .mluva
    @State private var samples: [Float] = Array(repeating: 0.04, count: 56)
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    var body: some View {
        Canvas { context, area in
            guard samples.count > 1 else { return }
            let step = area.width / CGFloat(samples.count - 1)
            let mid = area.height / 2
            var path = Path()
            for (index, sample) in samples.enumerated() {
                // A carrier wave whose height follows the voice, like a scope on a slow sweep.
                let wave = sin(Double(index) * 1.3) * Double(sample)
                let point = CGPoint(x: CGFloat(index) * step, y: mid - CGFloat(wave) * (area.height / 2 - 1))
                index == 0 ? path.move(to: point) : path.addLine(to: point)
            }
            context.stroke(path, with: .linearGradient(
                Gradient(colors: [color.opacity(0), color.opacity(0.9), color]),
                startPoint: .zero, endPoint: CGPoint(x: area.width, y: 0)), lineWidth: 1)
        }
        .onChange(of: level) { _, value in
            guard !reduceMotion else { return }
            samples.removeFirst()
            samples.append(min(1, max(0.04, value * 2)))
        }
        .accessibilityHidden(true)
    }
}

/// A mono label in small capitals-like caps with open tracking, for state words.
struct StateWord: View {
    let text: String
    var color: Color = .primary

    var body: some View {
        Text(text.uppercased())
            .font(.mono(10.5, .bold))
            .tracking(1.2)
            .foregroundStyle(color)
    }
}
