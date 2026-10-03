# Native Mluva third-party notices

Mluva is licensed under Apache-2.0; see `LICENSE`. `RUST-DEPENDENCIES.json` records the locked Cargo packages, declared licenses and archive checksums used to collect the accompanying `third-party-licenses/crates/` notices. It conservatively covers Cargo metadata's native workspace normal and build edges, including build tooling and resolved optional crates that the current binaries may not use. Each dependency retains its own license; no license is changed by inclusion in this inventory.

The linked Rust standard library is outside Cargo's application lockfile. Its complete upstream copyright report and license texts are retained under `third-party-licenses/rust-standard-library/`; the inventory records the pinned compiler version that supplies them. Those notices also cover standard-library components for targets this package does not use.

The bundled JetBrains Mono font files retain SIL OFL 1.1 in `resources/fonts/OFL.txt` and the included Quickshell font directories. Mermaid's JavaScript bundle retains its MIT license and provenance under `resources/mermaid/`.

The native ONNX feature/decoder implementation and embedded preprocessing data derive from onnx-asr 0.12.0 by Ilya Stupakov, under MIT. Its original notice is `third-party-licenses/onnx-asr/LICENSE`; upstream is https://github.com/istupakov/onnx-asr. Development tokenizer fixtures are excluded from this package.

The optional Tensaku editor remains a separately built/downloaded application. Its MPL-2.0 license, authorship notice, exact upstream source commit and complete narration patch are supplied under `linux/integrations/tensaku/`. Preserve the editor archive's additional dependency notices when distributing that editor.

GTK, Libadwaita, AT-SPI, GLib, Cairo, Pango, SQLite, OpenSSL, Fontconfig, PipeWire and optional WebKitGTK are system libraries/components managed by the Linux distribution. They are not copied into this runtime package. Optional speech models, Qwen and ONNX/CUDA runtimes are separately downloaded artifacts with their own pinned origins and notices. Codex and cloud providers are separately supplied applications/services.
