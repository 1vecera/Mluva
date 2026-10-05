# Current Wayland screenshot verification

6 October 2026: the exact installed Rust 2.1.0 application matches unchanged v1.6.0 for a saved conversation's real Omarchy region selection and fresh Tensaku narration on a private Wayland desktop. Earlier joined image checks used synthetic selection; this check runs the actual installed Omarchy scripts, Hyprpicker, Slurp and Grim. It adds evidence only and does not establish complete platform or physical-key parity.

The [compact receipts](receipts.json) preserve identities, all eight compared states, requests and full image hashes. The source is commit `5202477edfe4b5d8bacfa5b2e9fd6eadd9624f7f`; all 91 released Linux modules/resources are byte-verified against its Git archive before and after collection. A fresh private copy of the installed 2.1.0 bundle retains all 520 payload hashes and four links, verified before and after execution. Its app and narration helper are the actual packaged ELFs, not a rebuilt application harness. Both versions launch the same released Tensaku ELF, SHA-256 `79de63b635ec5a711bb250bb4fa7cd2dd69aeeb70dffe735a39c3669f7082831`.

## Observed behavior

The unchanged source configuration/storage APIs prepare only synthetic settings and one saved History item. An external observer launches each normal application, selects that conversation through `mluva-shell latest`, and sends virtual F10 through the private compositor's registered binding. The actual shell bridge asks the application to run `omarchy screenshot region save`. No application callback, recorder factory, screenshot service or editor implementation is replaced.

| State | Compared observation |
| --- | --- |
| Ready | One unchanged History row, no attachment and the original clipboard |
| Selection active | Actual Slurp and Hyprpicker, one private picker directory, no attachment |
| Selected | One image owned by the saved conversation; selector/freeze reaped and temporary directory gone |
| Narrating | Actual editor's chosen text area, native external PCM peer alive, one mode-0600 memory-backed WAV, no upload |
| Annotated | Exactly one speech upload, fresh phrase saved into the PNG, annotation processes/WAV gone |
| Cancel active | A second actual F10 picker; the saved annotation stays intact |
| Cancelled | Escape reaps selector/freeze and removes temporary files; no new image/upload |
| Closed | Normal application Quit, unchanged History/clipboard and saved annotation; external editor remains open as in v1.6.0 |

The private input driver drags from `(80, 140)` to `(720, 500)`, then moves the pointer outside the committed region before capture. Slurp includes the endpoint, yielding 641×361 pixels. The complete selected RGBA bytes equal the corresponding crop of the actual pre-picker frame in each version. Source and native selected PNG bytes also match exactly: 3,832 bytes, SHA-256 `1c12e9970ddc108e92596a705b36f16c2a6c79b49e6bc3ed0b7df145e32fe039`.

The observer accepts the editor's real first-use dialog, uses Add narration, drags a text area and presses Stop narration. Only the external audio/provider boundaries are synthetic: the existing native PCM peer emits 100 ms of fragmented signed 16-bit mono samples at 16 kHz; a private compatible HTTP endpoint returns `Private Wayland narration 82`. Both actual narration helpers upload the identical 3,244-byte WAV and model/language/response-format fields. Independent Tesseract OCR reads the fresh phrase from both exported images.

The complete annotated PNGs match exactly: 9,900 bytes, SHA-256 `a3427fc3715bd26626321de56af4fa5120b9dc0af429e71737c925e9017234b3`; their full RGBA hashes also match. The editor grows the canvas to 872×361 for the chosen text area and default text size in both versions. There is no pixel mask, output crop or native-derived reference.

![Independently captured narrated screenshot](annotated.png)

Every state checks the complete original History row is unchanged and the actual Wayland clipboard retains its sentinel. The screenshot remains owned by that History item with no capture owner or narration offset. Omarchy restores the cursor option's integer value; restoring it makes the option explicitly set, which is retained in raw observations. Normal Quit leaves the independent editor running; only subsequent private-namespace teardown ends it.

## Isolation and review

Fresh private HOME/XDG directories, session/accessibility buses, network/PID namespaces, `/run/q` and `/dev/shm` keep this work separate from the visible desktop. Headless Cage contains nested Hyprland with one 1280×900 output, Xwayland disabled and only `/dev/dri/renderD129` exposed. No host input/audio devices, display sockets, microphone, clipboard or provider credentials are used. Virtual keyboard/pointer clients connect only to that verified private compositor.

The native application and its descendants run with absolute Python, Python 3 and uv entry points shadowed by rejecting executables. Three positive invocations first verify those traps; their marker stays at exactly three through app startup, selection, editor narration, Escape and Quit. The reference preparation/observer remains a separate temporary Python process outside that app mount namespace and is neither maintained nor shipped.

The actual Omarchy scripts and system tool hashes are in the receipts. A transparent private `hyprpicker` adapter retains normally discarded diagnostics and execs `/usr/bin/hyprpicker` with the unchanged `-r -z` arguments. No Omarchy source or user configuration is modified. The checked packages are Omarchy 4.0.2, Hyprland 0.56.2, Hyprpicker 0.4.7, Grim/Slurp 1.5.0, imv 5.0.1 and wtype 0.4.

Initial observer failures are retained under ignored `tmp/current-wayland-screenshots/`: the Lua binding argument is a callback identifier, the virtual pointer needed a persistent seat device, and the nested app mount needed the already private device tree to keep `/dev/null` and the render node usable. Moving the pointer outside the selection removed cursor contamination from the expected frame. Independent review also corrected an assumed SQLite path column and expanded the WAV observation from `/run/q` to include its actual `/dev/shm` staging. These are excluded calibration failures, not Mluva repairs.

Final source evidence is `tmp/current-wayland-screenshots/source.FTaQyO/results.json` and native evidence is `tmp/current-wayland-screenshots/native.kJHjvF/results.json`. Their original sizes/hashes and the shared observer/input-driver fingerprints are in the receipts. `source-memory-audio.log` and `native-memory-audio.log` record successful terminal runs. A separate read-only audit checks the Git/source and installed/copy inventories, raw state equality, direct SQLite ownership, whole PNG/RGBA bytes, pre-picker crop, WAV samples, OCR, real tool identities, positive interpreter traps and normal shutdown. Its successful record is `tmp/current-wayland-screenshots/audit.json`; the audit SHA-256 is `c956a2787aea49a145769d39907763c8b6f9d69b97e5654336ca54a22042d402`.

Only documentation, compact receipts and two synthetic source images are tracked. Production, dependencies, native tests, existing frozen fixtures, installed settings and the released payload are unchanged. No new permanent owner, interpreter helper or bundled observer is added. This evidence-only change does not repeat the unchanged Cargo gates or claim a hosted CI run; CI remains manual `workflow_dispatch`.

## Remaining limits

Virtual F10 does not prove physical F10/F9. This saved-conversation flow does not exercise a simultaneous main recording, real microphone/provider, Wayland annotation Escape, every display scale/theme, GNOME or performance. The existing [joined image owner](../../../rust/mluva-gtk/tests/fixtures/application-images-evidence.md), [screenshot lifecycle owner](../../../rust/mluva-gtk/tests/fixtures/application-screenshots-evidence.md) and [narration/editor owner](../../../rust/mluva-workflows/tests/fixtures/narration-evidence.md) retain their separately bounded coverage. The [complete Rust acceptance matrix](../../rust-port-parity.md) stays open; this does not authorize or publish another release.
