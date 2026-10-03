# Owned screenshot picker

The immutable reference is Mluva v1.6.0 at `5202477edfe4b5d8bacfa5b2e9fd6eadd9624f7f`. [The independent fixture](released-screenshot-capture.json), SHA-256 `425307b4d4264cba873a232827fb1af3f14691f51d6be33f036b10052c061d49`, records 24 actual released `ScreenshotCapture.run` workflows. The external observer imports unchanged released modules and verifies their bytes against that commit. A separate native executable implements only the external `omarchy screenshot region save` boundary; it imports no Mluva implementation. Temporary reference collectors stay in ignored `tmp/`, and native checks require no interpreter.

Both implementations receive actual process output and read actual files. Observations compare exact PNG bytes, cancellation/error results, argument vectors, private directory mode, new process-group/session ownership, directory removal and eventual process disappearance. OS file failures are compared by errno rather than platform-specific prose. Generated private paths/PIDs are used to verify ownership and cleanup but are not frozen as expected literals. The fixture includes successful selections, relative paths, non-UTF-8 names, byte-whitespace trimming, stderr drainage, empty/nonzero results, oversized output, missing files/tools, directories, external/nested paths, symlinks, invalid/truncated/oversized images and cancellation before launch or with a child process.

The final case exercises the unchanged 180-second selection deadline in both implementations; it is not accelerated by a timeout override or virtual clock. The reference expired after 180.10 seconds and the native check after 180.01 seconds, each with its picker, child and temporary directory gone. These are timeout observations, not performance measurements. The deadline check is separate from the 23 short comparisons so routine process changes do not require repeatedly waiting three minutes.

Independent comparison first caught a bounded-output bug: retaining only 4,097 bytes made an oversized path with a whitespace prefix look like an empty selection. The native reader now drains the entire bounded-time stream while tracking whether any text occurred beyond its retained prefix. All-whitespace output still cancels exactly as the release does.

A native cancellation fault also reproduced a surviving child after its group leader exited on SIGTERM. Cleanup now observes leader exit without reaping it, signals the owned group before releasing that identity, and only then reaps the leader. A two-second grace period escalates an unresponsive group to SIGKILL. The runtime worker retains cleanup if its caller drops the waiting future. Four native faults verify the stubborn child, a dropped waiter with an unresponsive group, a dropped owner, and refusal to block on a FIFO returned as an image. These are additional lifecycle protections, not claims that the released implementation handles those faults identically. Cancellation and timeout cleanup are asynchronous.

Run the native checks in the existing private runner:

```sh
bash dev/run-isolated-browser.sh tmp/screenshot-picker-evidence -- \
  cargo test --locked -p mluva-workflows --test screenshot_capture -- \
  --ignored --test-threads=1 --nocapture
```

The tests require the runner's private HOME/XDG, network/PID namespaces and masked devices. Each native driver further clears its environment and uses only its private `bin` directory for PATH; the actual desktop picker can never be selected by these checks. The driver temporarily acts as a subreaper for synthetic grandchildren, leaving the production picker as the only owner that reaps its own leader. No real pixels, host clipboard/input, physical devices, credentials or network service are used. The native driver and picker peer are development binaries and must be excluded from distribution.

`ScreenshotCapture` is a native process/file owner. The separate [application screenshot comparison](../../../mluva-gtk/tests/fixtures/application-screenshots-evidence.md) now connects it to frozen attachment ownership, recording finalization, external editor monitoring and interrupted-image recovery. Native narrated boxes, actual private Wayland/Omarchy selection and complete application acceptance remain required. All full-workflow parity rows remain Pending; installed app/widget remain 1.6.0.
