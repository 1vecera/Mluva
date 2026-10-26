import Foundation
import ServiceManagement

/// Launch at login through the system's login items, which the person can see and revoke.
@MainActor
public enum LaunchAtLogin {
    public static var isEnabled: Bool { SMAppService.mainApp.status == .enabled }

    /// True when macOS wants approval in System Settings → General → Login Items.
    public static var needsApproval: Bool { SMAppService.mainApp.status == .requiresApproval }

    public static func set(_ enabled: Bool) throws {
        if enabled {
            try SMAppService.mainApp.register()
        } else {
            try SMAppService.mainApp.unregister()
        }
    }
}
