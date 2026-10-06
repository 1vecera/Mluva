# Collapsed Rewrite focus

Startup, Latest and the recorder's ordinary Open action still focused the manual rewrite text view after the requested 2.0.2 disclosure made it hidden by default. In the broader application comparison, native therefore retained an unmapped `GtkTextView` when the unchanged v1.6.0 reference focused the history button. The window's `focus-visible: false` value describes keyboard indication; it does not mean the reference button was hidden.

The two production focus branches now require the composer to be visible. Opening a conversation preserves the remembered disclosure choice, and an expanded composer retains its existing focus behavior. Live draft focus and explicit Rewrite commands are unchanged. The existing application owner checks actual mapped focus on startup and through the public Latest action. The existing review owner checks ordinary Open with both disclosure choices. No new test owner, source oracle, dependency or maintained Python is added.

The full `make linux-application-test` passes all 15 workflows/147 original GTK/store states, including the 22 Live states and four original layout/focus receipts, then passes the public command/settings owner. The review controller passes all 29 transactions/102 original states plus the new disclosure/focus assertions. Each new regression fails with its respective original focus branch restored; both branches were restored to the candidate before final checks.

| Retained log under `tmp/current-live-focus/` | Result | SHA-256 |
| --- | --- | --- |
| `startup-baseline.log` | Exit 101: startup focus is unmapped | `c695ef8d814ac4b67c227822826a2a8701b6d9bf2c885be9aac941f1bff9a231` |
| `review-baseline.log` | Exit 101: Open focuses the collapsed field | `702e9b492176304e773e37f0719859e46d0d40e51fcf3551eacc86e2a0f6d61f` |
| `application-final.log` | Exit 0: complete application and command owners | `305d53ec1ca151bd9459ed7a7317a6e3d554330e224dab311d28c7ef3ebf3bb4` |
| `review-candidate.log` | Exit 0: complete review owner | `b381518beba11095d51256872c74998a2df7467c82ac217d015dd442ef7bfa2f` |
| `native-gate.log` | Exit 0: 142 passed, 71 explicitly ignored | `7b532fcb0b9eb4817a9c17f457da4ad9d3c9d0b92e16321a2f6e4fdee8c950c1` |

The native gate includes both strict Clippy configurations, formatting and generated-feature consistency; ShellCheck also passes. The complete application run retains the existing transient GTK negative-width warning during continuation layout. Application/review registry logs contain only their successful startup line, with no dbind/cache failures. These use the declared private X11/Openbox runner with HOME/XDG, session/accessibility buses, network/PID namespaces and masked host input/audio devices. Its existing `ATSPI_DISABLE_P2P=1` setting remains explicit; this comparison does not establish default direct-transport browser behavior.

All four full Live frames are byte-identical to the independently retained untouched-release frames. ImageMagick `compare -metric AE` returns zero differing pixels for each, without masks. Source frames remain under `tmp/native-live-focus-source-evidence/session.z4IBl9/released-live-workspace-2dzpymw4/`; final native frames/layouts remain under `tmp/application/run.i7zHIm/session.Mqhw7I/native-application-live-workspace/`.

| Frame | Complete image dimensions | Shared reference/native PNG SHA-256 |
| --- | --- | --- |
| `grilling-wide` | 1060×780 | `b025094e7ba732a24a90a19aa8518059bca49d5925cb916711f0b93d7392bd31` |
| `grilling-sidebar` | 1060×780 | `9922579bab28b89441189ef49745432fe639b289c121e38d4a499dc18878a565` |
| `grilling-dark` | 1060×780 | `f3aa4232ecba85ea863ddb994179cf6ce461ecdaff450e334c59c86f9cce1c34` |
| `grilling-narrow` | 480×640 | `a9d33085a8d8f1b2f3e97905b43ba3a1ab8163c204b6cadfec2f0c9909b0d22d` |

The [Live fixture](../../../rust/mluva-gtk/tests/fixtures/released-application-live-workspace.json) remains SHA-256 `444fdff0eec2da4900aa3fd39d8110c6c4ea4062a86b856337c5f79ddb48684e`; the [review fixture](../../../rust/mluva-gtk/tests/fixtures/released-review-controller.json) remains `3e84076983446be587dab873ed4b6e56ad231e891d9857ebbcf2cdc01f253537`. Their reference stays v1.6.0 / `5202477edfe4b5d8bacfa5b2e9fd6eadd9624f7f`. Earlier [widget evidence](../current-wayland-widget/README.md) and its observations remain intact.

The installed 2.1.0 application, its manifest's 520 files/four links and PID 4173598 remain unchanged. App SHA-256 remains `4b15e162347c54ea5b5cf95dacea8fddc37f026256b72adec5480f6a2fe9e7df`; manifest remains `3bf35523cd8b329fee2d0d244c781dc467b3b3ef198bce9949ddbf619c155d51`. Physical F9/F10/microphone, installer hot-upgrade, browser/direct-transport privacy and GNOME/clean-distribution acceptance remain open. This is an unreleased source candidate, with no complete parity claim.
