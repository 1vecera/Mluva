// swift-tools-version: 6.2
import Foundation
import PackageDescription

// scripts/build-rust.sh writes the shared core's static library here first.
let rustProfile = Context.environment["MLUVA_RUST_PROFILE"] ?? "debug"
let rustLibrary = "\(Context.packageDirectory)/.rust/\(rustProfile)"

let package = Package(
    name: "Mluva",
    platforms: [.macOS("26.0")],
    products: [
        .executable(name: "Mluva", targets: ["Mluva"]),
    ],
    targets: [
        .systemLibrary(name: "CMluvaFFI", path: "Sources/CMluvaFFI"),
        .target(
            name: "MluvaKit",
            dependencies: ["CMluvaFFI"],
            swiftSettings: [.swiftLanguageMode(.v6)],
            linkerSettings: [
                .unsafeFlags(["-L", rustLibrary]),
                .linkedFramework("Security"),
                .linkedFramework("SystemConfiguration"),
                .linkedFramework("CoreFoundation"),
                .linkedLibrary("sqlite3"),
                .linkedLibrary("iconv"),
            ]
        ),
        .executableTarget(
            name: "Mluva",
            dependencies: ["MluvaKit"],
            swiftSettings: [.swiftLanguageMode(.v6)]
        ),
        .testTarget(
            name: "MluvaKitTests",
            dependencies: ["MluvaKit"],
            swiftSettings: [.swiftLanguageMode(.v6)]
        ),
    ]
)
