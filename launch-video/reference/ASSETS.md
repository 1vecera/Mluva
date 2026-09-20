# Asset provenance

| Asset | Source and treatment |
| --- | --- |
| `public/live/desktop.mp4` | Actual Omarchy desktop footage with production Mluva GTK, Chrome Guest, Ghostty and Herdr, plus new native Hyprland pickups below. Prepared microphone/provider outputs and private demonstration history. Edited to 84 seconds, 1920×1200 at 60 fps. Slowed scenes duplicate frames. |
| Setup pickup, film 0–18 s | Production GTK Welcome flow on an isolated native Hyprland compositor. Cloud/local speech, rewriting and recorder appearance. Real FT/VS Code/Ghostty/htop/Linear windows form the background layout. Compositor blur improves legibility over those windows. |
| Recorder pickup, film 30–44 s | Production Quickshell recorder on the same isolated Hyprland compositor. It genuinely tiles into the middle column, floats and pins again, and remains visible on desktop 2 with Firefox/Hacker News. Continuous native capture with animated wallpaper; no desktop still. Prepared transcript and running timer. |
| Layout and behavior references | User-supplied annotated desktop image and `screenrecording-2026-09-20_15-27-37.mp4`. Used to stage the arrangement and verify tile/float/pin behavior; neither reference is inserted into the film. |
| New desktop windows | Private Chromium profile on `https://www.ft.com/` and `https://linear.app/login`, private VS Code and Ghostty instances, htop, and private Firefox on `https://news.ycombinator.com/`. Linear shows its dark sign-in page, not a signed-in workspace. No personal browser profile or account content was copied. |
| Capture color metadata | wf-recorder marked limited-range YUV samples as full-range. Stream-copy `h264_metadata=video_full_range_flag=0` corrects that flag without changing encoded pixels. Decoded RGB samples match native PNG captures within 3 levels per channel. |
| Live rewrite/task, film 63–79 s | Retained actual host pickup incorporating the short-draft scroll correction from PR #45; retimed from the previous cut's 42–55-second section. |
| Recorded wallpaper | User-provided Downloads file `yq6gk3-x_LRGZ0fBO4Q7a_TJLfNl0x.mp4`, 15-second blue/gold star animation. Played beneath native windows during capture. Ownership or redistribution rights have not been independently established; it remains a local review input. |
| `public/live/sarah.wav` | Official Fish Audio Sarah, ID `933563129e564b19a115bedd57b7406a`, S2.1 Pro Free, speed 0.94. One energetic take with phoneme controls for Mluva and Live, normalized and placed in an 84-second 48 kHz mono PCM track. Script, voice identity, cue mapping and Scribe word timing are committed. |
| `public/audio/music-bed.wav` | User-selected `bombinsound-upbeat-background-music-version-5-rise-600001.mp3` from Downloads. Crossfaded to 84 seconds, normalized to −26 LUFS, 48 kHz stereo. No new generated music. The downloaded file's license has not been independently established; the film remains local. |
| `public/live/subtitles.srt` | Same text and timing as committed narration cues. Captions are burned into the rendered picture. The SRT stays in the media archive to avoid automatic duplicate playback subtitles. |
| `public/brand/mluva-logo-large-mark-on-dark.svg` | Exact repository lockup from `docs/brand/svg/`, used on the opening information overlay and closing card. |
| `public/fonts/JetBrainsMono-Regular.ttf` | Repository JetBrains Mono; SIL Open Font License included as `OFL.txt`. |
| `reference/{nord,tokyo-night,rose-pine}.toml` | Omarchy theme inputs applied by Mluva's production theme controller inside private capture state. |

Native F9 start/stop and Ctrl+V transfer into Chrome were captured on the actual host in the retained take. The new recorder pickup invokes the same native compositor operations as Super+T and Super+2 inside a separate compositor. Neither demonstrates automatic insertion into every application, real microphone recognition quality or service response speed. Grilling responses are prepared demonstration content. Native UI labels Live rewrite as Experimental.

Media and renders are local, excluded from the public source PR. `media.json` stores SHA-256 and probe details for the local v3 archive, which includes render inputs, source takes and production receipts. Browser profiles, cookies and runtime environments are excluded. The source PR does not publish the film or its downloaded assets.
