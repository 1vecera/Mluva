import MluvaKit
import SwiftUI

/// The F12 session wheel, drawn like a protractor: a ruled ring of ticks around a glass disc,
/// each running session on a leader line with its letter key, and the chosen one joined to
/// twelve o'clock by a red arc.
struct SessionWheelView: View {
    let model: AppModel
    /// Measured plate sizes by session; estimates stand in until a plate is measured.
    @State private var sizes: [String: CGSize] = [:]

    private var agents: AgentController { model.agents }
    private var layout: WheelLayout { WheelLayout(sessions: agents.sessions, page: model.wheelPage) }
    private var agentKey: String { agents.settings?.hold_key ?? "F12" }
    private var dictationKey: String { model.config?.global_recording_key ?? "F9" }
    static let ringRadius = 146.0
    static let discRadius = 118.0

    var body: some View {
        GeometryReader { proxy in
            let size = proxy.size
            let center = CGPoint(x: size.width / 2, y: size.height / 2)
            let itemSizes = layout.slots.map { sizes[$0.id] ?? CGSize(width: Double(label(for: $0).count) * 8 + 70, height: 40) }
            let offsets = layout.offsets(itemSizes: itemSizes, centerRadius: Self.ringRadius + 6, gap: 22,
                                         limit: Double(min(size.width, size.height)) / 2)
            ZStack {
                Color.black.opacity(0.42)
                    .ignoresSafeArea()
                    .contentShape(Rectangle())
                    .onTapGesture { model.closeWheel() }
                    .accessibilityHidden(true)
                Dial(slots: layout.slots, offsets: offsets, sizes: itemSizes, selected: selectedIndex)
                    .frame(width: size.width, height: size.height)
                    .allowsHitTesting(false)
                centerDisc
                    .frame(width: Self.discRadius * 2, height: Self.discRadius * 2)
                    .graphiteGlass(.circle)
                    .position(center)
                ForEach(Array(layout.slots.enumerated()), id: \.element.id) { index, slot in
                    plate(slot)
                        .onGeometryChange(for: CGSize.self) { $0.size } action: { sizes[slot.id] = $0 }
                        .position(x: center.x + (offsets.indices.contains(index) ? offsets[index].x : 0),
                                  y: center.y + (offsets.indices.contains(index) ? offsets[index].y : 0))
                }
                if layout.pageCount > 1 {
                    pager
                        .frame(maxHeight: .infinity, alignment: .bottom)
                        .padding(.bottom, 56)
                }
            }
            .frame(width: size.width, height: size.height)
        }
        .foregroundStyle(Ink.chalk)
        .environment(\.colorScheme, .dark)
        .onAppear(perform: installKeys)
    }

    private var selectedIndex: Int? {
        layout.slots.firstIndex(where: isSelected)
    }

    private func label(for slot: WheelLayout.Slot) -> String {
        slot.session.display_name + (slot.disambiguator.map { " \($0)" } ?? "")
    }

    private var selected: AgentSession? { agents.selected }

    private func isSelected(_ slot: WheelLayout.Slot) -> Bool {
        selected.map { $0.recipient.sameInstance(as: slot.session.recipient) } ?? false
    }

    private func plate(_ slot: WheelLayout.Slot) -> some View {
        let chosen = isSelected(slot)
        let session = slot.session
        return Button {
            model.choose(session)
        } label: {
            HStack(spacing: 9) {
                Keycap(String(slot.letter), lit: chosen, size: 11)
                VStack(alignment: .leading, spacing: 2) {
                    Text(session.display_name)
                        .font(.mono(12.5, chosen ? .bold : .medium))
                        .lineLimit(1)
                    HStack(spacing: 5) {
                        StatusMark(status: session.recipient.status, draftOnly: session.draft_only, size: 7)
                        Text(subtitle(slot))
                            .font(.mono(10))
                            .foregroundStyle(Ink.chalkDim)
                            .lineLimit(1)
                    }
                }
            }
            .padding(.leading, 7)
            .padding(.trailing, 12)
            .padding(.vertical, 7)
            .contentShape(.rect)
        }
        .buttonStyle(.plain)
        .graphiteGlass(.rect(cornerRadius: 6))
        .overlay(alignment: .bottom) {
            if chosen { Rectangle().fill(Color.mluva).frame(height: 1.5).padding(.horizontal, 1) }
        }
        .accessibilityLabel("\(session.display_name), \(statusText(session)), key \(String(slot.letter))")
        .accessibilityAddTraits(chosen ? .isSelected : [])
    }

    private func subtitle(_ slot: WheelLayout.Slot) -> String {
        let session = slot.session
        var parts = [session.draft_only ? "draft only" : session.recipient.status.word]
        if !session.project.isEmpty { parts.append(session.project) }
        if let handle = slot.disambiguator { parts.append(handle) }
        return parts.joined(separator: " · ")
    }

    private func statusText(_ session: AgentSession) -> String {
        session.draft_only ? session.draft_hint : session.recipient.status.word
    }

    @ViewBuilder private var centerDisc: some View {
        VStack(spacing: 7) {
            if let selected {
                JellyLight(mood: selected.draft_only ? .idle : .finishing, size: 22)
                Text(selected.display_name)
                    .font(.mono(15, .bold))
                    .multilineTextAlignment(.center)
                    .lineLimit(2)
                    .minimumScaleFactor(0.75)
                HStack(spacing: 5) {
                    StatusMark(status: selected.recipient.status, draftOnly: selected.draft_only, size: 7)
                    Text([selected.draft_only ? "draft only" : selected.recipient.status.word, selected.recipient.kind]
                        .filter { !$0.isEmpty }.joined(separator: " · "))
                        .font(.mono(10.5))
                        .foregroundStyle(Ink.chalkDim)
                }
                Rule(color: Ink.rule).frame(width: 120).padding(.vertical, 3)
                HStack(spacing: 6) {
                    Keycap(agentKey, lit: true)
                    Text(selected.draft_only ? "record draft" : "record").font(.mono(11))
                }
                HStack(spacing: 6) {
                    Keycap("esc")
                    Text("close").font(.mono(11)).foregroundStyle(Ink.chalkDim)
                }
            } else if agents.sessions.isEmpty {
                Text("no sessions")
                    .font(.mono(14, .bold))
                Text("Start Claude Code in a terminal. Mluva finds it on its own.")
                    .font(.mono(10.5))
                    .foregroundStyle(Ink.chalkDim)
                    .multilineTextAlignment(.center)
            } else {
                Text("choose a session")
                    .font(.mono(14, .bold))
                Text("Press its letter, or click it.")
                    .font(.mono(10.5))
                    .foregroundStyle(Ink.chalkDim)
                Rule(color: Ink.rule).frame(width: 120).padding(.vertical, 3)
                HStack(spacing: 6) {
                    Keycap(dictationKey)
                    Text("dictate a draft").font(.mono(11))
                }
            }
        }
        .padding(28)
    }

    private var pager: some View {
        HStack(spacing: 10) {
            Keycap("←").opacity(model.wheelPage > 0 ? 1 : 0.35)
            Text("page \(model.wheelPage + 1)/\(layout.pageCount)")
                .font(.mono(11))
                .monospacedDigit()
            Keycap("→").opacity(model.wheelPage + 1 < layout.pageCount ? 1 : 0.35)
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 7)
        .graphiteGlass(.rect(cornerRadius: 6))
        .accessibilityElement(children: .combine)
    }

    private func installKeys() {
        guard let panel = NSApp.windows.compactMap({ $0 as? FloatingPanel }).first(where: { $0.canBecomeKey }) else { return }
        panel.onKey = { event in
            MainActor.assumeIsolated {
                switch Int(event.keyCode) {
                case 53:
                    model.closeWheel()
                    return true
                case 123:
                    model.wheelPage = max(0, model.wheelPage - 1)
                    return true
                case 124:
                    model.wheelPage = min(layout.pageCount - 1, model.wheelPage + 1)
                    return true
                default:
                    guard let character = event.charactersIgnoringModifiers?.first,
                          let slot = layout.slot(for: character) else { return false }
                    model.choose(slot.session)
                    return true
                }
            }
        }
    }
}

/// The ruled ring: minute ticks, a node per session, leader lines out to each plate, and the
/// red arc from twelve o'clock to the chosen session.
private struct Dial: View {
    let slots: [WheelLayout.Slot]
    let offsets: [CGPoint]
    let sizes: [CGSize]
    let selected: Int?

    var body: some View {
        Canvas { context, area in
            let center = CGPoint(x: area.width / 2, y: area.height / 2)
            let radius = SessionWheelView.ringRadius
            func point(_ degrees: Double, _ r: Double) -> CGPoint {
                let radians = degrees * .pi / 180
                return CGPoint(x: center.x + r * sin(radians), y: center.y - r * cos(radians))
            }
            let rule = GraphicsContext.Shading.color(Ink.rule)
            context.stroke(Path(ellipseIn: CGRect(x: center.x - radius, y: center.y - radius, width: radius * 2, height: radius * 2)),
                           with: rule, lineWidth: 0.75)
            var ticks = Path()
            for tick in 0..<72 {
                let degrees = Double(tick) * 5
                let long = tick % 6 == 0
                ticks.move(to: point(degrees, radius))
                ticks.addLine(to: point(degrees, radius + (long ? 9 : 4)))
            }
            context.stroke(ticks, with: .color(Ink.chalkFaint), lineWidth: 0.75)
            if let selected, slots.indices.contains(selected) {
                var arc = Path()
                arc.addArc(center: center, radius: radius, startAngle: .degrees(-90),
                           endAngle: .degrees(slots[selected].angle - 90), clockwise: false)
                context.stroke(arc, with: .color(.mluva), style: StrokeStyle(lineWidth: 2, lineCap: .round))
            }
            for (index, slot) in slots.enumerated() where offsets.indices.contains(index) && sizes.indices.contains(index) {
                let node = point(slot.angle, radius)
                let plate = CGRect(x: center.x + offsets[index].x - sizes[index].width / 2,
                                   y: center.y + offsets[index].y - sizes[index].height / 2,
                                   width: sizes[index].width, height: sizes[index].height)
                // The leader runs out radially, then turns to meet the plate's nearest edge.
                let target = CGPoint(x: min(max(node.x, plate.minX), plate.maxX), y: min(max(node.y, plate.minY), plate.maxY))
                let elbow = point(slot.angle, radius + 16)
                var leader = Path()
                leader.move(to: node)
                leader.addLine(to: elbow)
                leader.addLine(to: target)
                let chosen = index == selected
                context.stroke(leader, with: chosen ? .color(.mluva) : .color(Ink.chalkFaint), lineWidth: chosen ? 1.25 : 0.75)
                let dot = chosen ? 4.5 : 2.5
                context.fill(Path(ellipseIn: CGRect(x: node.x - dot, y: node.y - dot, width: dot * 2, height: dot * 2)),
                             with: chosen ? .color(.mluva) : .color(Ink.chalk))
            }
        }
    }
}
