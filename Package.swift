// swift-tools-version: 5.10
import PackageDescription
import Foundation

let commandLineTestingLibraryPath = "/Library/Developer/CommandLineTools/Library/Developer/usr/lib"
let commandLineTestingLinkerSettings: [LinkerSetting] = FileManager.default.fileExists(
    atPath: "\(commandLineTestingLibraryPath)/lib_TestingInterop.dylib"
) ? [
    .unsafeFlags([
        "-L", commandLineTestingLibraryPath,
        "-Xlinker", "-rpath",
        "-Xlinker", commandLineTestingLibraryPath,
    ]),
] : []

let package = Package(
    name: "VoiceScribeMac",
    platforms: [.macOS(.v14)],
    dependencies: [
        .package(
            url: "https://github.com/swiftlang/swift-testing.git",
            revision: "swift-6.3.3-RELEASE"
        ),
        .package(url: "https://github.com/grpc/grpc-swift-2.git", from: "2.4.2"),
        .package(url: "https://github.com/grpc/grpc-swift-protobuf.git", from: "2.4.1"),
        .package(url: "https://github.com/grpc/grpc-swift-nio-transport.git", from: "2.9.0"),
        .package(url: "https://github.com/apple/swift-crypto.git", from: "4.5.1"),
    ],
    targets: [
        .executableTarget(
            name: "VoiceScribeMac",
            dependencies: [
                .product(name: "GRPCCore", package: "grpc-swift-2"),
                .product(
                    name: "GRPCNIOTransportHTTP2TransportServices",
                    package: "grpc-swift-nio-transport"
                ),
                .product(name: "GRPCProtobuf", package: "grpc-swift-protobuf"),
                .product(name: "CryptoExtras", package: "swift-crypto"),
            ],
            path: "Sources",
            linkerSettings: [
                .linkedFramework("AVFoundation"),
                .linkedFramework("CoreGraphics"),
                .linkedFramework("ApplicationServices"),
                .linkedFramework("Security"),
                .linkedFramework("ScreenCaptureKit"),
                .linkedFramework("Speech"),
            ],
            plugins: [
                .plugin(name: "GRPCProtobufGenerator", package: "grpc-swift-protobuf"),
            ]
        ),
        .testTarget(
            name: "VoiceScribeMacTests",
            dependencies: [
                "VoiceScribeMac",
                .product(name: "Testing", package: "swift-testing"),
            ],
            path: "Tests",
            linkerSettings: commandLineTestingLinkerSettings
        )
    ]
)
