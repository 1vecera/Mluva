import SwiftUI

struct MluvaApp: App {
    @NSApplicationDelegateAdaptor(AppDelegate.self) var appDelegate

    var body: some Scene {
        Settings {
            EmptyView()
        }
    }
}

@main
enum MluvaMain {
    @MainActor
    static func main() {
        do {
            try LegacyMigration.run()
        } catch {
            let alert = NSAlert()
            alert.messageText = "Mluva could not migrate existing data"
            alert.informativeText = "Your existing settings and recordings were preserved. See the identity migration guide before retrying."
            alert.alertStyle = .critical
            alert.runModal()
            exit(EXIT_FAILURE)
        }
        MluvaApp.main()
    }
}
