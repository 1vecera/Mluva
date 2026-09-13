# Third-party software

Mluva itself is available under the Apache License 2.0 in `LICENSE`. The Linux installer resolves the exact Python dependency versions in `linux/uv.lock`. Each dependency remains governed by its own license, and any future binary distributor must preserve the license and notice files shipped by the resolved package rather than treating this summary as a replacement.

## Linux runtime packages

The [Mluva wordmark](docs/assets/brand-source/wordmark-outline.svg) uses JetBrains Mono Medium outlines with optical spacing and a narrowed “l”. The unchanged Regular, Medium, Bold and Italic font binaries are bundled for app, recorder and diagram text under [SIL OFL 1.1](linux/quickshell/mluva.dictation/fonts/OFL.txt). [Font provenance](docs/assets/brand-source/wordmark-provenance.json) records the upstream revision and checksum. The former Adwaita Sans notice remains for the previously published launch-film assets. Figma symbol and production-media rights are documented in the [video-kit attribution](docs/design/video-kit/ATTRIBUTION.md).

| Package | Locked version | Upstream license | Project |
| --- | --- | --- | --- |
| dbus-next | 0.2.3 | MIT | [altdesktop/python-dbus-next](https://github.com/altdesktop/python-dbus-next) |
| websockets | 17.0.1 | BSD-3-Clause | [python-websockets/websockets](https://github.com/python-websockets/websockets) |

GTK, Libadwaita, PyGObject, WebKitGTK, AT-SPI, PipeWire, `wl-copy`, and the XDG desktop portal are operating-system components installed and updated through the distribution package manager rather than copied into this repository or installed from Mluva's lockfile. Codex is an optional, separately installed local application; cloud speech and rewrite providers are external services.

Review this inventory whenever the lockfile changes. The resolved packages remain the authoritative source for their complete license and notice files.

## Bundled diagram renderer

Mermaid 12.0.0 is bundled for local diagram rendering in `linux/resources/mermaid/mermaid.min.js`. It is distributed under the [MIT license](linux/resources/mermaid/LICENSE). The [bundle provenance](linux/resources/mermaid/README.md) records its upstream package and SHA-256 checksum; retain these files when redistributing the renderer.
