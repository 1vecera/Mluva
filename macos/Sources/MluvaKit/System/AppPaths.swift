import Foundation

/// Where Mluva keeps its files on macOS. The layout inside matches Linux's XDG directories.
public struct AppPaths: Sendable, Equatable {
    public var config: URL
    public var data: URL
    /// Herdr's configuration home, where its sockets live.
    public var herdrConfigHome: URL

    public init(config: URL, data: URL, herdrConfigHome: URL) {
        self.config = config
        self.data = data
        self.herdrConfigHome = herdrConfigHome
    }

    /// `~/Library/Application Support/Mluva`, or `MLUVA_HOME` for isolated runs and evidence.
    public static func standard(environment: [String: String] = ProcessInfo.processInfo.environment) -> AppPaths {
        let home = FileManager.default.homeDirectoryForCurrentUser
        let root = environment["MLUVA_HOME"].map { URL(fileURLWithPath: $0, isDirectory: true) }
            ?? home.appending(path: "Library/Application Support/Mluva", directoryHint: .isDirectory)
        let xdg = environment["XDG_CONFIG_HOME"].flatMap { $0.hasPrefix("/") ? $0 : nil }
            .map { URL(fileURLWithPath: $0, isDirectory: true) }
            ?? home.appending(path: ".config", directoryHint: .isDirectory)
        return AppPaths(
            config: root.appending(path: "config", directoryHint: .isDirectory),
            data: root.appending(path: "data", directoryHint: .isDirectory),
            herdrConfigHome: xdg)
    }

    public var recordings: URL { data.appending(path: "recordings", directoryHint: .isDirectory) }
    public var annotations: URL { data.appending(path: "annotations", directoryHint: .isDirectory) }
}
