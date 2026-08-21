# Third-party software

Mluva itself is available under the Apache License 2.0 in `LICENSE`. Mluva does not vendor the packages below in its source tree. The Linux installer resolves the exact Python dependency versions in `linux/uv.lock`; the macOS source preview resolves the exact Swift package versions and revisions in `Package.resolved`. Each dependency remains governed by its own license, and any future binary distributor must preserve the license and notice files shipped by the resolved package rather than treating this summary as a replacement.

## Linux runtime packages

| Package | Locked version | Upstream license | Project |
| --- | --- | --- | --- |
| dbus-next | 0.2.3 | MIT | [altdesktop/python-dbus-next](https://github.com/altdesktop/python-dbus-next) |
| websockets | 17.0.1 | BSD-3-Clause | [python-websockets/websockets](https://github.com/python-websockets/websockets) |

GTK, Libadwaita, PyGObject, AT-SPI, PipeWire, `wl-copy`, and the XDG desktop portal are operating-system components installed and updated through Fedora rather than copied into this repository or installed from Mluva's lockfile. Codex is an optional, separately installed local application; ElevenLabs and Google Cloud are external services.

## Direct macOS Swift packages

| Package | Resolved version | Upstream license | Project |
| --- | --- | --- | --- |
| grpc-swift-2 | 2.4.2 | Apache-2.0 | [grpc/grpc-swift-2](https://github.com/grpc/grpc-swift-2) |
| grpc-swift-nio-transport | 2.9.0 | Apache-2.0 | [grpc/grpc-swift-nio-transport](https://github.com/grpc/grpc-swift-nio-transport) |
| grpc-swift-protobuf | 2.4.1 | Apache-2.0 | [grpc/grpc-swift-protobuf](https://github.com/grpc/grpc-swift-protobuf) |
| swift-crypto | 4.5.1 | Apache-2.0 | [apple/swift-crypto](https://github.com/apple/swift-crypto) |
| swift-testing | swift-6.3.3-RELEASE | Apache-2.0 | [swiftlang/swift-testing](https://github.com/swiftlang/swift-testing) |

## Resolved transitive Swift packages

The current resolution also contains the following Apache-2.0 projects: `swift-algorithms` 1.2.1, `swift-asn1` 1.7.1, `swift-async-algorithms` 1.1.5, `swift-atomics` 1.3.1, `swift-certificates` 1.19.4, `swift-collections` 1.6.0, `swift-http-structured-headers` 1.7.0, `swift-http-types` 1.6.0, `swift-log` 1.14.0, `swift-nio` 2.101.3, `swift-nio-extras` 1.34.3, `swift-nio-http2` 1.45.0, `swift-nio-ssl` 2.37.2, `swift-nio-transport-services` 1.28.0, `swift-numerics` 1.1.1, `swift-protobuf` 1.38.1, `swift-service-lifecycle` 2.11.0, `swift-syntax` 603.0.2, and `swift-system` 1.7.5.

The resolved dependency archives are the authoritative source for their complete license and bundled third-party notices. This inventory should be regenerated and reviewed whenever either lockfile changes and before publishing a compiled macOS binary.
