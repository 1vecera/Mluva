import AppKit
import SwiftUI

extension Color {
    /// Mluva red: the one tint. Brighter on dark surfaces so it keeps its contrast.
    static let mluva = Color(nsColor: NSColor(name: "MluvaRed") { appearance in
        appearance.bestMatch(from: [.darkAqua, .vibrantDark, .accessibilityHighContrastDarkAqua]) != nil
            ? NSColor(srgbRed: 1.0, green: 0.251, blue: 0.282, alpha: 1)
            : NSColor(srgbRed: 0.914, green: 0.106, blue: 0.153, alpha: 1)
    })
}

/// What the jelly light is showing.
enum LightMood: Equatable {
    case idle
    case listening
    case finishing
    case done
    case attention
}

/// The brand's glossy red jelly, alive: it breathes on a 3.4-second cycle and swells with the
/// voice. Reduce Motion keeps its shape still and changes only its glow.
struct JellyLight: View {
    var mood: LightMood
    var level: Float = 0
    var size: CGFloat = 28

    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    var body: some View {
        TimelineView(.animation(minimumInterval: 1 / 60, paused: reduceMotion || mood == .idle || mood == .done)) { context in
            let t = context.date.timeIntervalSinceReferenceDate
            Canvas { canvas, area in
                draw(in: &canvas, area: area, time: reduceMotion ? 0 : t)
            }
        }
        .frame(width: size, height: size)
        .overlay {
            if mood == .done {
                Image(systemName: "checkmark")
                    .font(.system(size: size * 0.42, weight: .bold, design: .rounded))
                    .foregroundStyle(.white)
                    .transition(.scale.combined(with: .opacity))
            }
        }
        .accessibilityElement()
        .accessibilityLabel(accessibilityText)
    }

    private var accessibilityText: String {
        switch mood {
        case .idle: "Ready"
        case .listening: "Recording"
        case .finishing: "Finishing"
        case .done: "Done"
        case .attention: "Needs attention"
        }
    }

    private func draw(in canvas: inout GraphicsContext, area: CGSize, time t: Double) {
        let center = CGPoint(x: area.width / 2, y: area.height / 2)
        let breath = 0.5 + 0.5 * sin(t * 2 * .pi / 3.4)
        let voice = mood == .listening ? Double(min(1, level)) : 0
        let base = Double(min(area.width, area.height)) / 2 * 0.80
        let radius: Double = switch mood {
        case .listening: base * (0.90 + 0.05 * breath + 0.10 * voice)
        case .finishing: base * (0.86 + 0.03 * breath)
        default: base * 0.92
        }
        // A soft, irregular contour like the brand mark: low harmonics drifting slowly.
        let wobble = mood == .listening ? 0.035 + 0.05 * voice : (mood == .finishing ? 0.02 : 0.025)
        let speed = mood == .finishing ? 0.35 : 0.9
        var path = Path()
        let steps = 96
        for step in 0...steps {
            let angle = Double(step) / Double(steps) * 2 * .pi
            let r = radius * (1
                + wobble * sin(2 * angle + t * speed * 1.3 + 0.6)
                + wobble * 0.7 * sin(3 * angle - t * speed * 1.7 + 1.9)
                + wobble * 0.4 * sin(5 * angle + t * speed * 2.3))
            let point = CGPoint(x: center.x + r * cos(angle), y: center.y + r * sin(angle) * 0.96)
            step == 0 ? path.move(to: point) : path.addLine(to: point)
        }
        path.closeSubpath()
        let dim = mood == .finishing ? 0.82 : 1.0
        // Depth under the jelly.
        var shadow = canvas
        shadow.addFilter(.blur(radius: radius * 0.22))
        shadow.opacity = 0.45 * dim
        shadow.fill(path.offsetBy(dx: 0, dy: radius * 0.16), with: .color(Color(red: 0.55, green: 0.02, blue: 0.06)))
        // Body: lit from the upper left like the glossy mark.
        canvas.opacity = dim
        canvas.fill(path, with: .radialGradient(
            Gradient(stops: [
                .init(color: Color(red: 1.0, green: 0.42, blue: 0.44), location: 0),
                .init(color: Color(red: 0.93, green: 0.10, blue: 0.15), location: 0.48),
                .init(color: Color(red: 0.62, green: 0.03, blue: 0.08), location: 1),
            ]),
            center: CGPoint(x: center.x - radius * 0.32, y: center.y - radius * 0.38),
            startRadius: 0, endRadius: radius * 1.55))
        // Rim light along the lower edge.
        canvas.stroke(path, with: .linearGradient(
            Gradient(colors: [.clear, Color(red: 1, green: 0.55, blue: 0.55).opacity(0.55)]),
            startPoint: CGPoint(x: center.x, y: center.y - radius), endPoint: CGPoint(x: center.x, y: center.y + radius)),
            lineWidth: max(0.8, radius * 0.06))
        // Specular highlight.
        let highlight = Path(ellipseIn: CGRect(
            x: center.x - radius * 0.58, y: center.y - radius * 0.72, width: radius * 0.62, height: radius * 0.36))
        var gloss = canvas
        gloss.addFilter(.blur(radius: radius * 0.06))
        gloss.fill(highlight, with: .linearGradient(
            Gradient(colors: [.white.opacity(0.92), .white.opacity(0.0)]),
            startPoint: CGPoint(x: center.x, y: center.y - radius * 0.72),
            endPoint: CGPoint(x: center.x, y: center.y - radius * 0.30)))
        let glint = Path(ellipseIn: CGRect(
            x: center.x + radius * 0.30, y: center.y + radius * 0.28, width: radius * 0.16, height: radius * 0.11))
        canvas.fill(glint, with: .color(.white.opacity(0.55)))
    }
}

/// Status dot colours for agent sessions, from the system palette.
extension AgentStatusPresentation {
    static func color(_ status: String) -> Color {
        switch status {
        case "idle", "done": .green
        case "working": .blue
        case "blocked": .orange
        default: .secondary
        }
    }
}

enum AgentStatusPresentation {}
