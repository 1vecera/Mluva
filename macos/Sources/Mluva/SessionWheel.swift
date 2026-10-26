import MluvaKit
import SwiftUI

/// The F12 session wheel: running agent sessions on a ring of glass, chosen by letter key.
struct SessionWheelView: View {
    let model: AppModel
    @Namespace private var glass

    private var agents: AgentController { model.agents }
    private var layout: WheelLayout { WheelLayout(sessions: agents.sessions, page: model.wheelPage) }
    private var agentKey: String { agents.settings?.hold_key ?? "F12" }
    private var dictationKey: String { model.config?.global_recording_key ?? "F9" }

    var body: some View {
        GeometryReader { proxy in
            let size = proxy.size
            let widest = (layout.slots.map { Double(label(for: $0).count) * 8.4 }.max() ?? 120) + 72
            let radius = layout.radius(itemWidth: widest, centerRadius: 118, limit: Double(min(size.width, size.height)) / 2 - 60)
            ZStack {
                Color.black.opacity(0.35)
                    .ignoresSafeArea()
                    .contentShape(Rectangle())
                    .onTapGesture { model.closeWheel() }
                    .accessibilityHidden(true)
                GlassEffectContainer(spacing: 12) {
                    ZStack {
                        center
                            .frame(width: 236, height: 236)
                            .glassEffect(.regular, in: .circle)
                            .glassEffectID("center", in: glass)
                        ForEach(layout.slots) { slot in
                            item(slot)
                                .offset(
                                    x: radius * sin(slot.angle * .pi / 180),
                                    y: -radius * cos(slot.angle * .pi / 180))
                        }
                    }
                }
                if layout.pageCount > 1 {
                    pager
                        .frame(maxHeight: .infinity, alignment: .bottom)
                        .padding(.bottom, 64)
                }
            }
            .frame(width: size.width, height: size.height)
        }
        .onAppear(perform: installKeys)
    }

    private func label(for slot: WheelLayout.Slot) -> String {
        slot.session.display_name + (slot.disambiguator.map { " \($0)" } ?? "")
    }

    private var selected: AgentSession? { agents.selected }

    private func isSelected(_ slot: WheelLayout.Slot) -> Bool {
        selected.map { $0.recipient.sameInstance(as: slot.session.recipient) } ?? false
    }

    private func item(_ slot: WheelLayout.Slot) -> some View {
        let chosen = isSelected(slot)
        let session = slot.session
        return Button {
            model.choose(session)
        } label: {
            HStack(spacing: 10) {
                Text(String(slot.letter))
                    .font(.system(.callout, design: .rounded).weight(.semibold))
                    .frame(width: 24, height: 24)
                    .background(.quaternary, in: .rect(cornerRadius: 6))
                VStack(alignment: .leading, spacing: 0) {
                    HStack(spacing: 6) {
                        if session.draft_only {
                            Image(systemName: "square")
                                .font(.caption2.weight(.bold))
                                .foregroundStyle(.secondary)
                                .accessibilityLabel("Draft only")
                        } else {
                            Circle()
                                .fill(AgentStatusPresentation.color(session.recipient.status.rawValue))
                                .frame(width: 7, height: 7)
                                .accessibilityHidden(true)
                        }
                        Text(session.display_name)
                            .font(.body.weight(chosen ? .semibold : .regular))
                    }
                    Text(subtitle(slot))
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
            }
            .padding(.leading, 8)
            .padding(.trailing, 16)
            .padding(.vertical, 8)
        }
        .buttonStyle(.plain)
        .glassEffect(chosen ? .regular.tint(.mluva.opacity(0.55)).interactive() : .regular.interactive(), in: .capsule)
        .glassEffectID(slot.id, in: glass)
        .accessibilityLabel("\(session.display_name), \(statusText(session)), key \(slot.letter)")
        .accessibilityAddTraits(chosen ? .isSelected : [])
    }

    private func subtitle(_ slot: WheelLayout.Slot) -> String {
        var parts = [slot.session.project]
        if let handle = slot.disambiguator { parts.append(handle) }
        if slot.session.draft_only { parts.append("draft only") }
        return parts.filter { !$0.isEmpty }.joined(separator: " · ")
    }

    private func statusText(_ session: AgentSession) -> String {
        if session.draft_only { return session.draft_hint }
        switch session.recipient.status {
        case .idle: return "ready"
        case .working: return "working"
        case .blocked: return "needs attention"
        case .done: return "finished a turn"
        case .unknown: return "status unknown"
        }
    }

    @ViewBuilder private var center: some View {
        VStack(spacing: 8) {
            if let selected {
                JellyLight(mood: selected.draft_only ? .idle : .finishing, size: 30)
                Text(selected.display_name)
                    .font(.system(.title2, design: .rounded).weight(.semibold))
                    .multilineTextAlignment(.center)
                    .lineLimit(3)
                    .minimumScaleFactor(0.7)
                Text(selected.draft_only ? selected.draft_hint : statusText(selected).capitalized)
                    .font(.callout)
                    .foregroundStyle(.secondary)
                    .multilineTextAlignment(.center)
                Text(selected.draft_only ? "\(agentKey) record draft · Esc close" : "\(agentKey) record · Esc close")
                    .font(.caption.weight(.medium))
                    .foregroundStyle(.secondary)
                    .padding(.top, 2)
            } else if agents.sessions.isEmpty {
                Image(systemName: "sparkles.rectangle.stack")
                    .font(.title)
                    .foregroundStyle(.secondary)
                Text("No agent sessions")
                    .font(.system(.title3, design: .rounded).weight(.semibold))
                Text("Start Claude Code in a terminal. Mluva finds it on its own.")
                    .font(.callout)
                    .foregroundStyle(.secondary)
                    .multilineTextAlignment(.center)
            } else {
                Text("Choose a session")
                    .font(.system(.title3, design: .rounded).weight(.semibold))
                Text("Press its letter, or click it.")
                    .font(.callout)
                    .foregroundStyle(.secondary)
                Text("\(dictationKey) dictates a draft · Esc close")
                    .font(.caption.weight(.medium))
                    .foregroundStyle(.secondary)
            }
        }
        .padding(26)
    }

    private var pager: some View {
        HStack(spacing: 12) {
            Image(systemName: "chevron.left").opacity(model.wheelPage > 0 ? 1 : 0.3)
            Text("Page \(model.wheelPage + 1) of \(layout.pageCount)")
                .font(.callout.monospacedDigit())
            Image(systemName: "chevron.right").opacity(model.wheelPage + 1 < layout.pageCount ? 1 : 0.3)
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 8)
        .glassEffect(.regular, in: .capsule)
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
