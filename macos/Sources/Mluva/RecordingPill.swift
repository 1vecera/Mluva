import MluvaKit
import SwiftUI

/// The floating recording display: a ruled sheet of live words over a title-block strip with
/// the signal light, a state word, the timer, where the words go, and the keys that act.
struct RecordingPillView: View {
    let model: AppModel

    private var dictation: DictationController { model.dictation }
    private var route: RouteState? { model.agents.route }
    private var key: String { model.config?.global_recording_key ?? "F9" }
    private var draftKey: String { model.agents.settings?.draft_key ?? "F7" }

    var body: some View {
        VStack(spacing: 8) {
            Spacer(minLength: 0)
            if let transcript, !transcript.isEmpty {
                TranscriptCard(text: transcript, committedCount: dictation.committed.count, live: dictation.phase == .recording)
                    .graphiteGlass(.rect(cornerRadius: 10))
                    .transition(.opacity.combined(with: .offset(y: 6)))
            }
            strip
                .graphiteGlass(.rect(cornerRadius: 10))
        }
        .foregroundStyle(Ink.chalk)
        .animation(.smooth(duration: 0.3), value: transcript?.isEmpty ?? true)
        .animation(.smooth(duration: 0.25), value: stateKey)
        .padding(.horizontal, 24)
        .padding(.bottom, 8)
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .bottom)
    }

    /// Live words while recording, the final text afterwards; nothing when there are none.
    private var transcript: String? {
        switch dictation.phase {
        case .recording, .finalizing: dictation.previewText
        case .finished: route?.text.isEmpty == false ? route?.text : dictation.lastText
        default: nil
        }
    }

    private var stateKey: String {
        "\(dictation.phase)|\(route?.phase ?? "")"
    }

    /// Cells divided by hairlines, like a drawing's title block.
    private var strip: some View {
        VStack(spacing: 0) {
            HStack(spacing: 0) {
                HStack(spacing: 9) {
                    JellyLight(mood: mood, level: dictation.level, size: 20)
                    StateWord(text: stateWord, color: stateColor)
                    if dictation.phase == .recording, let started = dictation.startedAt {
                        ElapsedTime(since: started)
                    }
                }
                .padding(.horizontal, 12)
                .fixedSize()
                Rule(vertical: true, color: Ink.rule)
                VStack(alignment: .leading, spacing: 2) {
                    Text(headline)
                        .font(.mono(12.5, .medium))
                        .lineLimit(1)
                    Text(detail)
                        .font(.mono(10.5))
                        .foregroundStyle(Ink.chalkDim)
                        .lineLimit(2)
                        .fixedSize(horizontal: false, vertical: true)
                }
                .padding(.horizontal, 12)
                .frame(maxWidth: .infinity, alignment: .leading)
                .accessibilityElement(children: .combine)
                actions
                    .padding(.trailing, 10)
            }
            .padding(.vertical, 9)
            footer
                .frame(height: 14)
                .padding(.horizontal, 12)
                .padding(.bottom, 6)
        }
        .frame(width: 540)
        .fixedSize(horizontal: false, vertical: true)
    }

    /// The live trace while recording; the cancel window as a shrinking dimension line.
    @ViewBuilder private var footer: some View {
        if route?.phase == "grace", let deadline = route?.deadline_ms {
            CountdownLine(deadline: deadline)
        } else if dictation.phase == .recording {
            LevelTrace(level: dictation.level)
        } else {
            Rule(color: Ink.rule)
        }
    }

    @ViewBuilder private var actions: some View {
        HStack(spacing: 10) {
            if route?.phase == "grace" {
                KeyAction(key: draftKey, label: "keep draft") { model.agents.disarm("kept") }
                PlainAction(label: "cancel") { model.agents.disarm("cancelled") }
                    .accessibilityLabel("Cancel sending")
            } else if dictation.phase == .recording || dictation.phase == .preparing {
                KeyAction(key: key, label: "stop", lit: true) { Task { await dictation.stop() } }
                    .disabled(dictation.phase == .preparing)
                    .accessibilityLabel("Stop and deliver")
                PlainAction(label: "cancel") { dictation.cancel() }
                    .accessibilityLabel("Cancel recording")
            } else if case .failed = dictation.phase {
                PlainAction(label: "dismiss") { dictation.reset() }
            }
        }
    }

    private var mood: LightMood {
        if route?.phase == "unconfirmed" { return .attention }
        if route?.phase == "grace" || route?.phase == "submitting" { return .finishing }
        if route?.phase == "sent" { return .done }
        switch dictation.phase {
        case .recording: return .listening
        case .preparing, .finalizing: return .finishing
        case .finished: return route?.phase == "draft" ? .idle : .done
        case .failed: return .attention
        default: return .idle
        }
    }

    private var stateWord: String {
        switch route?.phase {
        case "grace": return "send"
        case "submitting": return "sending"
        case "sent": return "sent"
        case "unconfirmed": return "check"
        case "draft": return "draft"
        default: break
        }
        switch dictation.phase {
        case .idle: return "ready"
        case .preparing: return "arming"
        case .recording: return dictation.incognito ? "rec · incog" : "rec"
        case .finalizing: return "reading"
        case .finished: return finishedWord
        case .noSpeech: return "silence"
        case .failed: return "stopped"
        }
    }

    private var stateColor: Color {
        if route?.phase == "unconfirmed" { return Ink.amber }
        if case .failed = dictation.phase { return Ink.amber }
        return dictation.phase == .recording || route?.phase == "grace" ? .mluva : Ink.chalk
    }

    private var finishedWord: String {
        if case .finished(let guidance) = dictation.phase {
            if guidance.hasPrefix("Pasted") { return "pasted" }
            if guidance.hasPrefix("Saved") { return "saved" }
        }
        return "copied"
    }

    private var agentName: String { route?.recipient.title.isEmpty == false ? route!.recipient.title : (route?.recipient.name ?? "the agent") }

    private var headline: String {
        switch route?.phase {
        case "grace", "submitting": return "→ \(agentName)"
        case "sent": return "→ \(agentName)"
        case "unconfirmed": return "Delivery unconfirmed"
        default: break
        }
        switch dictation.phase {
        case .idle: return "\(key) to dictate"
        case .preparing, .recording: return "→ \(destinationName)"
        case .finalizing: return "Recognizing what you said"
        case .finished: return route?.phase == "draft" ? "Kept as a draft" : "→ \(destinationName)"
        case .noSpeech: return "No speech detected"
        case .failed: return "Couldn't finish"
        }
    }

    private var destinationName: String {
        switch dictation.destination {
        case .target(let target): target?.applicationName ?? "clipboard"
        case .manual: "clipboard"
        case .agent(let session): session.display_name
        case .document(let session): session.map { "draft for \($0.display_name)" } ?? "Mluva draft"
        }
    }

    private var detail: String {
        switch route?.phase {
        case "grace": return "Sends when the line runs out."
        case "submitting": return "Writing into the session."
        case "sent": return "Herdr confirmed the message reached the session."
        case "unconfirmed": return "Some text may have arrived. Check the session; Mluva won't resend."
        case "draft": return draftReason
        default: break
        }
        switch dictation.phase {
        case .preparing: return "Opening the microphone"
        case .recording:
            if let notice = model.annotation.notice { return notice }
            return model.annotation.active ? "Hold shift to mark the screen" : destinationDetail
        case .finalizing: return "Final words from Scribe"
        case .finished(let guidance): return dictation.notice ?? model.annotation.notice ?? guidance
        case .noSpeech: return "Nothing was copied or saved."
        case .failed(let message): return message
        case .idle: return "Press \(key) again to stop"
        }
    }

    private var destinationDetail: String {
        switch dictation.destination {
        case .target(let target): target == nil ? "Copied when you stop" : "Pasted when you stop"
        case .manual: "Copied when you stop"
        case .agent: "Sent after a two-second cancel window"
        case .document: "An editable draft; nothing is sent"
        }
    }

    private var draftReason: String {
        switch route?.reason {
        case "kept": "In History and ready to edit or send."
        case "cancelled": "Sending was cancelled. The text is in History."
        case "unavailable": "The session became unavailable. Choose another and send."
        case "rejected": "The session refused it. Nothing was written there."
        case "empty": "Nothing was recognized."
        default: "The text is kept in History."
        }
    }
}

/// A shortcut drawn as its keycap beside what it does; clicking does the same.
private struct KeyAction: View {
    let key: String
    let label: String
    var lit = false
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            HStack(spacing: 6) {
                Keycap(key, lit: lit)
                Text(label).font(.mono(11))
            }
            .contentShape(.rect)
        }
        .buttonStyle(.plain)
    }
}

/// An action with no shortcut: just its word, underlined on hover.
private struct PlainAction: View {
    let label: String
    let action: () -> Void
    @State private var hovering = false

    var body: some View {
        Button(action: action) {
            Text(label)
                .font(.mono(11))
                .foregroundStyle(hovering ? Ink.chalk : Ink.chalkDim)
                .underline(hovering)
                .contentShape(.rect)
        }
        .buttonStyle(.plain)
        .onHover { hovering = $0 }
    }
}

/// Five lines at most, newest at the bottom; volatile words are dimmed.
private struct TranscriptCard: View {
    let text: String
    let committedCount: Int
    let live: Bool

    var body: some View {
        ScrollViewReader { reader in
            ScrollView {
                styled
                    .font(.mono(13.5))
                    .lineSpacing(7)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(.horizontal, 18)
                    .padding(.vertical, 14)
                    .id("end")
            }
            .scrollIndicators(.never)
            .defaultScrollAnchor(.bottom)
            .onChange(of: text) { reader.scrollTo("end", anchor: .bottom) }
        }
        .frame(width: 540)
        .frame(maxHeight: 5 * 22 + 28)
        .fixedSize(horizontal: false, vertical: true)
        .accessibilityLabel(text)
    }

    private var styled: Text {
        guard live, committedCount < text.count else { return Text(text) }
        let split = text.index(text.startIndex, offsetBy: committedCount)
        return Text("\(Text(text[..<split]))\(Text(text[split...]).foregroundStyle(Ink.chalkFaint))")
    }
}

private struct ElapsedTime: View {
    let since: Date

    var body: some View {
        TimelineView(.periodic(from: since, by: 1)) { context in
            let seconds = max(0, Int(context.date.timeIntervalSince(since)))
            Text(String(format: "%02d:%02d", seconds / 60, seconds % 60))
                .font(.mono(12.5, .medium))
                .monospacedDigit()
                .accessibilityLabel("Elapsed \(seconds / 60) minutes \(seconds % 60) seconds")
        }
    }
}

/// The two-second cancel window as a red dimension line shrinking toward its end mark, with
/// the time left set beside it.
private struct CountdownLine: View {
    let deadline: UInt64

    var body: some View {
        TimelineView(.animation) { context in
            let remaining = max(0, Double(deadline) / 1_000 - context.date.timeIntervalSince1970)
            GeometryReader { proxy in
                let width = proxy.size.width - 44
                let mid = proxy.size.height / 2
                ZStack(alignment: .leading) {
                    Path { path in
                        path.move(to: CGPoint(x: 0, y: mid - 4)); path.addLine(to: CGPoint(x: 0, y: mid + 4))
                        path.move(to: CGPoint(x: width, y: mid - 4)); path.addLine(to: CGPoint(x: width, y: mid + 4))
                    }
                    .stroke(Ink.rule, lineWidth: 1)
                    Path { path in
                        path.move(to: CGPoint(x: 0, y: mid))
                        path.addLine(to: CGPoint(x: width * min(1, remaining / 2), y: mid))
                    }
                    .stroke(Color.mluva, lineWidth: 1.5)
                    Text(String(format: "%.1fs", remaining))
                        .font(.mono(10, .medium))
                        .monospacedDigit()
                        .foregroundStyle(Color.mluva)
                        .offset(x: width + 8)
                }
            }
        }
        .accessibilityHidden(true)
    }
}
