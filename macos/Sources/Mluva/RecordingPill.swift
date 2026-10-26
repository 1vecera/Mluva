import MluvaKit
import SwiftUI

/// The floating recording display: a glass capsule with the jelly light, what is happening,
/// where the words go, and the live transcript growing out of it.
struct RecordingPillView: View {
    let model: AppModel
    @Namespace private var glass

    private var dictation: DictationController { model.dictation }
    private var route: RouteState? { model.agents.route }

    var body: some View {
        VStack(spacing: 10) {
            Spacer(minLength: 0)
            GlassEffectContainer(spacing: 14) {
                VStack(spacing: 10) {
                    if let transcript, !transcript.isEmpty {
                        TranscriptCard(text: transcript, committedCount: dictation.committed.count, live: dictation.phase == .recording)
                            .glassEffect(.regular, in: .rect(cornerRadius: 22))
                            .glassEffectID("transcript", in: glass)
                            .transition(.opacity.combined(with: .scale(scale: 0.96, anchor: .bottom)))
                    }
                    capsule
                        .glassEffect(.regular, in: .capsule)
                        .glassEffectID("capsule", in: glass)
                }
            }
            .animation(.smooth(duration: 0.35), value: transcript?.isEmpty ?? true)
            .animation(.smooth(duration: 0.3), value: stateKey)
        }
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

    private var capsule: some View {
        HStack(spacing: 14) {
            ZStack {
                if route?.phase == "grace", let deadline = route?.deadline_ms {
                    CountdownRing(deadline: deadline)
                }
                JellyLight(mood: mood, level: dictation.level, size: 34)
            }
            .frame(width: 44, height: 44)
            VStack(alignment: .leading, spacing: 1) {
                Text(headline)
                    .font(.headline)
                    .lineLimit(1)
                Text(detail)
                    .font(.subheadline)
                    .foregroundStyle(.secondary)
                    .lineLimit(2)
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .accessibilityElement(children: .combine)
            if dictation.phase == .recording, let started = dictation.startedAt {
                ElapsedTime(since: started)
            }
            actions
        }
        .padding(.leading, 12)
        .padding(.trailing, 10)
        .padding(.vertical, 9)
        .frame(minWidth: 380)
        .fixedSize(horizontal: false, vertical: true)
    }

    @ViewBuilder private var actions: some View {
        if route?.phase == "grace" {
            Button("Keep draft") { model.agents.disarm("kept") }
                .buttonStyle(.glass)
            Button(role: .cancel) { model.agents.disarm("cancelled") } label: {
                Image(systemName: "xmark")
            }
            .buttonStyle(.glass)
            .buttonBorderShape(.circle)
            .accessibilityLabel("Cancel sending")
        } else if dictation.phase == .recording || dictation.phase == .preparing {
            Button { Task { await dictation.stop() } } label: {
                Image(systemName: "stop.fill")
                    .font(.system(size: 13, weight: .bold))
                    .foregroundStyle(.white)
                    .frame(width: 34, height: 34)
                    .contentShape(.circle)
            }
            .buttonStyle(.plain)
            // Solid, because a panel that never takes focus shows tinted glass as inactive.
            .background(Color.mluva.gradient, in: .circle)
            .shadow(color: Color.mluva.opacity(0.35), radius: 6, y: 2)
            .disabled(dictation.phase == .preparing)
            .opacity(dictation.phase == .preparing ? 0.5 : 1)
            .accessibilityLabel("Stop and deliver")
            Button(role: .cancel) { dictation.cancel() } label: {
                Image(systemName: "xmark")
            }
            .buttonStyle(.glass)
            .buttonBorderShape(.circle)
            .controlSize(.large)
            .accessibilityLabel("Cancel recording")
        } else if case .failed = dictation.phase {
            Button("Dismiss") { dictation.reset() }
                .buttonStyle(.glass)
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

    private var agentName: String { route?.recipient.title.isEmpty == false ? route!.recipient.title : (route?.recipient.name ?? "the agent") }

    private var headline: String {
        switch route?.phase {
        case "grace": return "Sending to \(agentName)"
        case "submitting": return "Sending to \(agentName)…"
        case "sent": return "Sent to \(agentName)"
        case "unconfirmed": return "Delivery unconfirmed"
        default: break
        }
        switch dictation.phase {
        case .idle: return "Ready"
        case .preparing: return "Getting ready…"
        case .recording: return dictation.incognito ? "Listening · Incognito" : "Listening"
        case .finalizing: return "Finishing…"
        case .finished: return route?.phase == "draft" ? "Kept as a draft" : finishedHeadline
        case .noSpeech: return "No speech detected"
        case .failed: return "Couldn't finish"
        }
    }

    private var finishedHeadline: String {
        if case .finished(let guidance) = dictation.phase {
            if guidance.hasPrefix("Pasted") { return "Pasted" }
            if guidance.hasPrefix("Saved") { return "Saved in History" }
        }
        return "Copied"
    }

    private var detail: String {
        switch route?.phase {
        case "grace": return "Edit, Keep draft or Cancel to stop it."
        case "sent": return "Herdr confirmed the message reached the session."
        case "unconfirmed": return "Some text may have arrived. Check the session; Mluva won't resend."
        case "draft": return draftReason
        default: break
        }
        switch dictation.phase {
        case .preparing: return dictation.destination.label
        case .recording:
            if let notice = model.annotation.notice { return "\(dictation.destination.label) · \(notice)" }
            return model.annotation.active ? "\(dictation.destination.label) · Shift draws on the screen" : dictation.destination.label
        case .finalizing: return "Recognizing what you said"
        case .finished(let guidance): return dictation.notice ?? model.annotation.notice ?? guidance
        case .noSpeech: return "Nothing was copied or saved."
        case .failed(let message): return message
        case .idle: return "Press F9 to dictate"
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

/// Five lines at most, newest at the bottom; volatile words are dimmed.
private struct TranscriptCard: View {
    let text: String
    let committedCount: Int
    let live: Bool

    var body: some View {
        ScrollViewReader { reader in
            ScrollView {
                styled
                    .font(.body)
                    .lineSpacing(3)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(.horizontal, 18)
                    .padding(.vertical, 14)
                    .id("end")
            }
            .scrollIndicators(.never)
            .defaultScrollAnchor(.bottom)
            .onChange(of: text) { reader.scrollTo("end", anchor: .bottom) }
        }
        .frame(maxHeight: 5 * 22 + 28)
        .fixedSize(horizontal: false, vertical: true)
        .accessibilityLabel(text)
    }

    private var styled: Text {
        guard live, committedCount < text.count else { return Text(text) }
        let split = text.index(text.startIndex, offsetBy: committedCount)
        return Text("\(Text(text[..<split]))\(Text(text[split...]).foregroundStyle(.secondary))")
    }
}

private struct ElapsedTime: View {
    let since: Date

    var body: some View {
        TimelineView(.periodic(from: since, by: 1)) { context in
            let seconds = max(0, Int(context.date.timeIntervalSince(since)))
            Text(String(format: "%d:%02d", seconds / 60, seconds % 60))
                .font(.system(.title3, design: .rounded).monospacedDigit())
                .foregroundStyle(.secondary)
                .accessibilityLabel("Elapsed \(seconds / 60) minutes \(seconds % 60) seconds")
        }
    }
}

/// The two-second cancel window draining around the light.
private struct CountdownRing: View {
    let deadline: UInt64

    var body: some View {
        TimelineView(.animation) { context in
            let remaining = max(0, Double(deadline) / 1_000 - context.date.timeIntervalSince1970)
            Circle()
                .trim(from: 0, to: remaining / 2)
                .stroke(Color.mluva, style: StrokeStyle(lineWidth: 3, lineCap: .round))
                .rotationEffect(.degrees(-90))
                .padding(1.5)
        }
        .accessibilityHidden(true)
    }
}
