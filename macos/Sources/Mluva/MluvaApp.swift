import AppKit
import MluvaKit
import SwiftUI

@main
struct MluvaApp: App {
    @NSApplicationDelegateAdaptor(AppDelegate.self) private var delegate
    @State private var model = AppModel()
    @State private var historySelection: String?

    var body: some Scene {
        MenuBarExtra {
            MenuBarView(model: model)
        } label: {
            MenuBarIcon(model: model, recording: model.dictation.phase == .recording)
        }
        .menuBarExtraStyle(.window)

        WindowGroup("History", id: "history", for: String.self) { $value in
            HistoryView(model: model, selection: Binding(
                get: { historySelection ?? (value?.isEmpty == false ? value : nil) },
                set: { historySelection = $0 }))
                .frame(minWidth: 720, minHeight: 460)
        }
        .defaultSize(width: 900, height: 600)

        Window("Welcome to Mluva", id: "welcome") {
            OnboardingView(model: model)
        }
        .windowResizability(.contentSize)
        .windowStyle(.hiddenTitleBar)

        Settings {
            SettingsView(model: model)
        }
    }
}

/// The menu bar mark: the brand silhouette, filled while recording.
struct MenuBarIcon: View {
    let model: AppModel
    let recording: Bool
    @Environment(\.openWindow) private var openWindow
    @Environment(\.openSettings) private var openSettings

    var body: some View {
        icon.onAppear {
            model.openWindow = { id, value in
                if let value { openWindow(id: id, value: value) } else { openWindow(id: id) }
            }
            model.openSettingsAction = { openSettings() }
            Evidence.run(model)
            if !model.isEvidence, model.config?.welcome_completed != true { openWindow(id: "welcome"); NSApp.activate() }
        }
    }

    @ViewBuilder private var icon: some View {
        if let image = Self.mark {
            Image(nsImage: image)
                .opacity(recording ? 1 : 0.85)
                .accessibilityLabel(recording ? "Mluva, recording" : "Mluva")
        } else {
            Image(systemName: recording ? "waveform.circle.fill" : "waveform")
        }
    }

    static let mark: NSImage? = {
        guard let url = Bundle.main.url(forResource: "MenuBarMark", withExtension: "svg"),
              let image = NSImage(contentsOf: url) else { return nil }
        image.size = NSSize(width: 18, height: 18)
        image.isTemplate = true
        return image
    }()
}

final class AppDelegate: NSObject, NSApplicationDelegate {
    func applicationDidFinishLaunching(_ notification: Notification) {
        NSApp.setActivationPolicy(.accessory)
    }
}
