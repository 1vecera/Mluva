# Native desktop status bridge

`mluva-shell` is a headless Rust executable for the existing Omarchy/widget session-bus protocol. It accepts `watch`, all six application actions and all six review operations, including optional saved-style identifiers. Normal status contains only phase and elapsed time. `--overlay` explicitly enables the bounded volatile preview and review controls. The executable does not initialize GTK, read application stores or start Mluva.

The immutable reference is released v1.6.0 at `5202477edfe4b5d8bacfa5b2e9fd6eadd9624f7f`. [The process fixture](released-shell.json), SHA-256 `e432b4db0766e4f04ba441a6f6382f9e33cd55f71970cf141ae3378e36d001b4`, comes from unchanged released shell-bridge processes. The temporary external observer verifies every released Python module against that commit. It only normalizes the reference program name to `mluva-shell` before invoking the unchanged module, so help and error output use the installed command name.

The maintained native test owns the executable boundary. It copies the ELF into an otherwise empty private directory, changes working directory and removes DISPLAY, WAYLAND_DISPLAY, AT_SPI_BUS_ADDRESS, PYTHONPATH and VIRTUAL_ENV from its children. Independent peers expose the real `org.gtk.Actions` interface on separate connections. No production injection hook or maintained Python helper is used.

The frozen comparison covers 37 CLI cases, 21 real action receipts, 107 watcher steps and two disconnect paths. CLI checks compare exit codes and exact stdout/stderr, including option abbreviations, argument placement, invalid review operations and missing application ownership. Action receipts independently check exactly one invocation, unique-owner destinations, the actual `NO_AUTO_START` message flag, parameter tuples and empty platform data. A nonresponding peer proves the released 1.5-second call timeout; a rejecting peer proves bounded, redacted diagnostics.

Watcher observations cover initial absence, synchronous status replay, active/hidden/review/unknown phases, wrong signatures, elapsed/settings limits, NaN/infinities, Unicode whitespace, the final 4,096 preview scalars, 128 review options and identifier/label/message bounds. Wrong path/interface/signal and a still-connected former owner cannot update the stream. Replacing an owner clears the previous state, subscribes before replay and observes the new sender; owner exit and failed replay clear or mark status as unavailable. JSON records are compared as parsed values, with ASCII escaping separately required; no byte-identical float-formatting claim is made.

A dedicated private bus has an actual activatable `com.mluva.Linux` service. Neither Watch nor Record starts it. An explicit `StartServiceByName` then creates an independent marker, proving that this negative check had a working activation route. The reference and native results both match. The trap is confined to the disposable network/PID/session environment and never contacts the host service manager.

There is one deliberate cleanup repair: when the widget closes its output pipe, v1.6.0 exits 120 and prints a `BrokenPipeError` during interpreter finalization. The native bridge exits 0 with empty diagnostics. Session-bus disappearance retains Gio's released SIGTERM behavior. All other compared stderr output matches; all successful native watcher logs are empty. HOME/config/data/state directories stay empty throughout the native check. The linked bridge uses Gio/GLib and no GTK or Python library.

Build and run explicitly inside the guarded environment:

```sh
cargo test --locked -p mluva-shell --no-run
bash dev/run-isolated-browser.sh tmp/native-shell -- \
  cargo test --locked -p mluva-shell --test bridge -- \
  --ignored --test-threads=1 --nocapture
```

The runner isolates display, session/accessibility buses, HOME/XDG, network/PID/mount namespaces and host input/audio/GPU devices. The bridge itself runs without display variables. The test is ignored by ordinary Cargo runs because it requires these boundaries. Raw reference evidence is retained under ignored `tmp/shell-reference-reviewed-evidence/`; the accepted native rerun and workspace-check paths are recorded in the parity checkpoint.

The review caught an observer race: a correct initial `stopped` line could arrive before the test sampled its per-step starting length. The maintained observer now keeps one cursor from the beginning of the stream and deliberately waits for the first real line before starting its assertions. This also rejects unexpected late messages between steps. The failed `tmp/shell-reviewed-acceptance.log` is diagnostic evidence only; no production behavior or frozen output was changed to accommodate it.

This establishes the native command and its bounded protocol behavior. Final package selection, managed-credential launcher integration, a real Omarchy/Wayland widget and GNOME acceptance, complete joined workflows and performance comparisons remain required. The existing Python launcher/package and installed v1.6.0 remain in place until full replacement acceptance; all complete-workflow parity rows remain Pending.
