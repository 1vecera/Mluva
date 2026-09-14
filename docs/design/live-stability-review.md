# Live dictation and identity study — September 14, 2026

Fresh dictation now begins at the top of the source pane. When Live rewrite is enabled, the draft pane appears before the first provider response and keeps its geometry when that response arrives. New words are fully opaque immediately. Native GTK commits changed ranges in one buffer transaction; the recorder widget only gives an existing-word correction a 3 px, 180 ms upward settling motion.

The jump had two causes: the source follower did not retain its implicit Gtk.TextView reference, so resetting a recording failed to clear its correction inset; repeated partial updates also accumulated temporary bottom padding. The follower now resets the actual view, ignores identical updates and reserves space from the intended scroll destination. Existing manual-scroll recovery after large recognition contractions remains covered.

The recording light uses a cosine size curve and a squared-sine deformation envelope. At both extremes, deformation and its velocity are zero, leaving a small or large circle at the same centre. The Blender study follows those endpoint principles. All 102 rendered poses are present in the native Figma loop, with exactly one opaque pose active at a time.

| Verification | Result |
| --- | --- |
| `make linux-test` | 440 tests passed; Ruff check and format passed. One existing PyGObject deprecation warning. |
| `make linux-live-stability-test` | Passed with native animations enabled in Fedora/Xvfb and the Omarchy ARM VM. |
| Live regression observations | Draft mapped before response; source margin 4 px; maximum reverse scroll 0 px during 48 incremental updates; 100 identical updates did not inflate the extent; second recording reset to the top. |
| `feedback_ui_smoke.py` through `dev/run-isolated.sh` | Passed: exact replacement, manual edits, contraction tail visibility and release of correction padding on manual scrolling. |
| `make linux-omarchy-test` | Passed for the actual Quickshell widget, including correction flow, wrapping, themes, review controls and placement. |
| `make linux-shortcut-test` | Private portal registration, binding and shortcut lifecycle passed. |
| `make linux-fluid-workspace-test` in Omarchy | Passed with WebKit sandboxing enabled. |
| ShellCheck | Installer, Linux scripts, test launchers, development scripts and `mluva-shell` passed. |
| Figma layout and source audit | 15 pages, 121 variables, 81 components, 13 screens, 9 timelines / 395 tracks. No board overlaps, missing fonts or visible auto-layout overflow. The seven-line recorder transcript intentionally exceeds its clipped five-line viewport. |
| Figma variable stress | Three layout controls changed together without overflow. Timing changes updated 374 tracks; restoring values returned a zero-change dry run across both motion pages. |
| Logo source checks | Ten independent concept authors, ten logo/wordmark pairs, exact PNG hashes and actual transparent alpha. |

The offscreen launcher now gives WebKit a real, short, private runtime directory. Its former symlink could point outside WebKit’s sandbox bind mount and prevent the D-Bus proxy from launching. Logs move back into the worktree evidence directory after the private session exits; dead sockets are excluded. No host desktop input was used.

Adversarial review removed the draft-wide and word-arrival fades, checked the first-response and next-recording boundaries, aligned the paired boxes in the motion studies, normalized logo comparison sizes, and replaced a ghosted four-pose breathing crossfade with the complete sequence. The updated loop was exported and sampled after correcting its layer positioning. The source verifier checks the complete pose coverage mathematically, including every transition boundary and interval midpoint.

The design sources and test receipts are review evidence, not provider-accuracy or latency measurements. The ten identities remain study options. This change does not assemble the final film or choose a production logo.
