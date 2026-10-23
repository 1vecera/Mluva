# Raw measurements

Everything below was measured on the guest's own clocks, on 23 October 2026 unless noted.

## Shift press to the portal signal

`evtest` on the QEMU USB keyboard stamped each Shift press (kernel time). `dbus-monitor --profile` stamped the portal's `Activated` signal. Five presses:

| Press (s) | `Activated` (s) | Delay |
| --- | --- | --- |
| 1792758127.409237 | 1792758127.409769 | 0.53 ms |
| 1792758128.372043 | 1792758128.372765 | 0.72 ms |
| 1792758129.399457 | 1792758129.399988 | 0.53 ms |
| 1792758130.427397 | 1792758130.427865 | 0.47 ms |
| 1792758131.455342 | 1792758131.455824 | 0.48 ms |

## Shift press to Mluva's pointer query

`strace -tt -e connect` on Mluva (debug build) recorded its connection to Hyprland's command socket after each of three Shift presses:

| Press | Connection | Delay |
| --- | --- | --- |
| 12:26:18.615545 | 12:26:18.620963 | 5.4 ms |
| 12:26:19.860135 | 12:26:19.897010 | 36.9 ms |
| 12:26:21.157540 | 12:26:21.200839 | 43.3 ms |

## Where strokes begin

Each line ran 400 px in 33 steps of 12 ms. It started at local x 80 on Virtual-2 (scale 1.5) and at x 300 on Virtual-1 (scale 1), so one step is 12.1 px. "Delay" is the time from Shift down to the first motion. The table gives local x of the first recorded point, then of the next point (the first motion the overlay received).

| Build | Delay | Virtual-2 | Virtual-1 |
| --- | --- | --- | --- |
| `2f85360` (before the lead point), debug | 0 ms | 165, 153 (first motion only) | 372, 372 |
| `2f85360`, debug | 50 ms | 129, 116 | 324, 336 |
| `2f85360`, debug | 200, 600 or 1,500 ms | 92 (step 1) every time | 312 (step 1) every time |
| `df075c5`, debug | 0 ms | 129 then 153; 141 then 165 | 360 then 372; 360 then 384 |
| `df075c5`, release | 0 ms, 3 runs | 129–141 then 165 | 360 then 372–384 |
| `df075c5`, release, `GSK_RENDERER=cairo` | 0 ms, 3 runs | 141 then 153 | 348–360 then 360–372 |
| `189f948` (final), debug | see [starts/strokes.jsonl](starts/strokes.jsonl) | 141–153 at 0 ms; 92 at 50 ms | 360–372 at 0 ms; 312–324 at 50 ms |

Guest CPU load (16 busy loops, 98 % CPU) thinned a two-output circle from 66 recorded points to 37, but its start was intact. The original two-output run on 21 October lost about 110 ms at the start and two thirds of its samples. That run overlapped a 20-job host build.

## Shift release after a chord

`dbus-monitor` on the portal's `GlobalShortcuts` signals:

| Sequence | Before `189f948` | With `189f948` |
| --- | --- | --- |
| Type `W` (Shift, w) | `Activated` only | `Activated`, then `Deactivated` 24 ms later |
| Shift-click (Shift, left button) | `Activated` (ink), `Activated` (pin), `Deactivated` (pin) | the same, then `Deactivated` (ink) at the Shift release |
| Shift alone | `Activated`, `Deactivated` | `Activated`, `Deactivated` |

Before the fix, plain pointer motion after typing a capital drew ink: 1,576 red pixels in the probe window ([screenshot](before-fixes/ink-stuck-after-capital.png)). With the fix there were 0 after a capital and 0 after a pin.
