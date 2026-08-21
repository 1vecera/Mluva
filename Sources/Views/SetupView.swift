import AppKit
import SwiftUI
import UniformTypeIdentifiers

struct SetupView: View {
    @EnvironmentObject var permissionsManager: PermissionsManager

    @State private var stage: SetupStage = .provider
    @State private var providerPreference = AppSettings.shared.providerPreference
    @State private var cloudRecognitionAllowed = AppSettings.shared.cloudRecognitionAllowed
    @State private var googleCloudProjectID = AppSettings.shared.googleCloudProjectID
    @State private var googleServiceAccountFilePath = AppSettings.shared.googleServiceAccountFilePath
    @State private var googleCredentialError: String?

    private enum SetupStage: CaseIterable {
        case provider
        case microphone
        case speechRecognition
        case accessibility
    }

    var body: some View {
        VStack(spacing: 16) {
            Spacer().frame(height: 4)

            Text("Set up Mluva")
                .font(.headline)

            stageIndicator

            Divider()

            switch stage {
            case .provider:
                providerStage
            case .microphone:
                microphoneStage
            case .speechRecognition:
                speechRecognitionStage
            case .accessibility:
                accessibilityStage
            }

            Spacer().frame(height: 4)
        }
        .padding(.horizontal, 16)
        .onAppear {
            providerPreference = AppSettings.shared.providerPreference
            cloudRecognitionAllowed = AppSettings.shared.cloudRecognitionAllowed
            googleCloudProjectID = AppSettings.shared.googleCloudProjectID
            googleServiceAccountFilePath = AppSettings.shared.googleServiceAccountFilePath
            stage = AppSettings.shared.hasCompletedSetup ? nextRequiredStage : .provider
            permissionsManager.startPolling()
        }
        .onDisappear {
            permissionsManager.stopPolling()
        }
    }

    private var visibleStages: [SetupStage] {
        if !needsAppleSpeechPermission {
            return [.provider, .microphone, .accessibility]
        }
        return SetupStage.allCases
    }

    private var stageIndicator: some View {
        HStack(spacing: 8) {
            ForEach(visibleStages, id: \.self) { item in
                Capsule()
                    .fill(item == stage ? Color.accentColor : Color.secondary.opacity(0.25))
                    .frame(width: item == stage ? 20 : 8, height: 8)
            }
        }
        .animation(.easeInOut(duration: 0.15), value: stage)
    }

    private var providerStage: some View {
        VStack(spacing: 12) {
            Image(systemName: "waveform.badge.mic")
                .font(.title)
                .foregroundColor(.accentColor)

            Text("Choose recognition")
                .font(.subheadline.bold())

            Picker("Provider", selection: $providerPreference) {
                Text("Automatic").tag(TranscriptionProviderKind.automatic)
                Text("Apple on-device").tag(TranscriptionProviderKind.apple)
                Text("Google Cloud").tag(TranscriptionProviderKind.googleCloud)
            }
            .pickerStyle(.segmented)
            .onChange(of: providerPreference) { _, value in
                AppSettings.shared.providerPreference = value
            }

            Text(providerExplanation)
                .font(.caption)
                .foregroundColor(.secondary)
                .multilineTextAlignment(.center)

            if providerPreference != .apple {
                Toggle("Allow audio to Google Cloud", isOn: $cloudRecognitionAllowed)
                    .font(.callout)
                    .onChange(of: cloudRecognitionAllowed) { _, value in
                        AppSettings.shared.cloudRecognitionAllowed = value
                    }

                TextField("Google Cloud project ID", text: $googleCloudProjectID)
                    .textFieldStyle(.roundedBorder)
                    .onChange(of: googleCloudProjectID) { _, value in
                        AppSettings.shared.googleCloudProjectID = value
                    }

                HStack {
                    Text(googleAuthenticationLabel)
                        .font(.caption2)
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                        .truncationMode(.middle)
                    Spacer()
                    Button("Choose service account…") {
                        chooseGoogleServiceAccountFile()
                    }
                    .font(.caption2)
                    if !googleServiceAccountFilePath.isEmpty {
                        Button("Use ADC") {
                            googleServiceAccountFilePath = ""
                            AppSettings.shared.googleServiceAccountFilePath = ""
                            googleCredentialError = nil
                        }
                        .font(.caption2)
                    }
                }

                Text("Uses Application Default Credentials from gcloud unless you explicitly select a service-account JSON. Only the file path is saved.")
                    .font(.caption2)
                    .foregroundColor(.secondary)
                    .multilineTextAlignment(.center)

                if let googleCredentialError {
                    Text(googleCredentialError)
                        .font(.caption2)
                        .foregroundStyle(.red)
                }
            }

            Button("Continue") {
                advance()
            }
            .buttonStyle(.borderedProminent)
            .disabled(!providerConfigurationIsValid)
        }
    }

    private var microphoneStage: some View {
        VStack(spacing: 12) {
            Image(systemName: permissionsManager.microphoneGranted ? "mic.fill" : "mic.slash")
                .font(.title)
                .foregroundColor(permissionsManager.microphoneGranted ? .green : .accentColor)

            Text("Microphone access")
                .font(.subheadline.bold())

            Text("Audio is captured locally before the selected recognizer receives it.")
                .font(.caption)
                .foregroundColor(.secondary)
                .multilineTextAlignment(.center)

            if permissionsManager.microphoneGranted {
                Label("Granted", systemImage: "checkmark.circle.fill")
                    .foregroundColor(.green)
                Button("Continue") { advance() }
                    .buttonStyle(.borderedProminent)
            } else {
                Button("Grant microphone access") {
                    permissionsManager.requestMicrophone()
                }
                .buttonStyle(.borderedProminent)
            }
        }
    }

    private var speechRecognitionStage: some View {
        VStack(spacing: 12) {
            Image(systemName: permissionsManager.speechRecognitionGranted
                  ? "text.bubble.fill" : "text.bubble")
                .font(.title)
                .foregroundColor(permissionsManager.speechRecognitionGranted ? .green : .accentColor)

            Text("Apple Speech access")
                .font(.subheadline.bold())

            Text("Required for private on-device recognition. Google-only mode does not need this permission.")
                .font(.caption)
                .foregroundColor(.secondary)
                .multilineTextAlignment(.center)

            if permissionsManager.speechRecognitionGranted {
                Label("Granted", systemImage: "checkmark.circle.fill")
                    .foregroundColor(.green)
                Button("Continue") { advance() }
                    .buttonStyle(.borderedProminent)
            } else {
                Button("Grant speech recognition access") {
                    permissionsManager.requestSpeechRecognition()
                }
                .buttonStyle(.borderedProminent)
            }
        }
    }

    private var accessibilityStage: some View {
        VStack(spacing: 12) {
            Image(systemName: permissionsManager.accessibilityGranted
                  ? "hand.raised.fill" : "hand.raised")
                .font(.title)
                .foregroundColor(permissionsManager.accessibilityGranted ? .green : .accentColor)

            Text("Type into every app")
                .font(.subheadline.bold())

            Text("Accessibility enables the global hotkey and automatic insertion. Without it, finished text is copied to the clipboard.")
                .font(.caption)
                .foregroundColor(.secondary)
                .multilineTextAlignment(.center)

            if permissionsManager.accessibilityGranted {
                Label("Granted", systemImage: "checkmark.circle.fill")
                    .foregroundColor(.green)
                Button("Finish setup") { finishSetup() }
                    .buttonStyle(.borderedProminent)
            } else {
                Button("Open System Settings") {
                    permissionsManager.requestAccessibility()
                }
                .buttonStyle(.borderedProminent)

                Button("Use clipboard mode") {
                    finishSetup()
                }
                .buttonStyle(.plain)
                .font(.caption)
                .foregroundColor(.secondary)
            }
        }
    }

    private var providerExplanation: String {
        switch providerPreference {
        case .automatic:
            return "Uses Apple privately when available and Google only when cloud use is allowed and requested."
        case .apple:
            return "Keeps recognition on this Mac through Apple Speech."
        case .googleCloud:
            return "Uses Google Speech-to-Text V2 with your own Google Cloud project."
        }
    }

    private var providerConfigurationIsValid: Bool {
        guard providerPreference == .googleCloud else { return true }
        return cloudRecognitionAllowed
            && !googleCloudProjectID.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
    }

    private var googleAuthenticationLabel: String {
        guard !googleServiceAccountFilePath.isEmpty else {
            return "Authentication: Application Default Credentials"
        }
        return "Authentication: \(URL(fileURLWithPath: googleServiceAccountFilePath).lastPathComponent)"
    }

    private func chooseGoogleServiceAccountFile() {
        let panel = NSOpenPanel()
        panel.allowedContentTypes = [.json]
        panel.allowsMultipleSelection = false
        panel.canChooseDirectories = false
        panel.begin { response in
            guard response == .OK, let url = panel.url else { return }
            do {
                _ = try GoogleServiceAccountCredential.load(from: url)
                googleServiceAccountFilePath = url.path
                AppSettings.shared.googleServiceAccountFilePath = url.path
                googleCredentialError = nil
            } catch {
                googleCredentialError = error.localizedDescription
            }
        }
    }

    private var cloudFallbackAvailable: Bool {
        cloudRecognitionAllowed
            && !googleCloudProjectID.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
    }

    private var needsAppleSpeechPermission: Bool {
        switch providerPreference {
        case .apple:
            return true
        case .googleCloud:
            return false
        case .automatic:
            return !cloudFallbackAvailable
        }
    }

    private var nextRequiredStage: SetupStage {
        if !permissionsManager.microphoneGranted {
            return .microphone
        }
        if needsAppleSpeechPermission, !permissionsManager.speechRecognitionGranted {
            return .speechRecognition
        }
        return .accessibility
    }

    private func advance() {
        guard let index = visibleStages.firstIndex(of: stage),
              visibleStages.indices.contains(index + 1)
        else {
            finishSetup()
            return
        }
        withAnimation {
            stage = visibleStages[index + 1]
        }
    }

    private func finishSetup() {
        AppSettings.shared.hasCompletedSetup = true
    }
}
