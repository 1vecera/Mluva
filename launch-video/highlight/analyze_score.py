#!/usr/bin/env python3
"""Measure a generated score: tempo, beat offset, loudness envelope and the biggest low-band drops.

usage: uv run --with numpy highlight/analyze_score.py score.mp3 [--json]
"""
import json, subprocess, sys
import numpy as np

SR = 22050
path = sys.argv[1]
pcm = subprocess.run(["ffmpeg", "-v", "error", "-i", path, "-ac", "1", "-ar", str(SR), "-f", "f32le", "-"], capture_output=True, check=True).stdout
x = np.frombuffer(pcm, dtype=np.float32)
hop = 441  # 20 ms
n = len(x) // hop
frames = x[: n * hop].reshape(n, hop)
rms = np.sqrt((frames ** 2).mean(1))
spec = np.abs(np.fft.rfft(x[: n * hop].reshape(n, hop) * np.hanning(hop), axis=1))
freqs = np.fft.rfftfreq(hop, 1 / SR)
low = spec[:, freqs < 160].sum(1)
flux = np.maximum(0, np.diff(np.log1p(spec * 20), axis=0)).sum(1)
t = np.arange(n) * hop / SR

# Tempo: autocorrelation of the onset (flux) envelope between 100 and 160 BPM.
env = flux - flux.mean()
ac = np.correlate(env, env, "full")[len(env) - 1 :]
lags = np.arange(len(ac)) * hop / SR
best = max(range(int(0.375 / (hop / SR)), int(0.6 / (hop / SR))), key=lambda i: ac[i])
beat = lags[best]
# Beat offset: phase of the strongest low-band onsets against that grid.
phases = np.arange(0, 1, 0.01)
score = [sum(low[(np.round((t0 + phase * beat) / (hop / SR)) + np.arange(0, n, beat / (hop / SR))).astype(int).clip(0, n - 1)]) for phase in phases for t0 in [0]]
offset = float(phases[int(np.argmax(score))] * beat)

# Drops: a low-band jump over the previous second, smoothed, local maxima at least 3 s apart.
w = int(1.0 / (hop / SR))
smooth = np.convolve(low, np.ones(w // 4) / (w // 4), "same")
jump = np.array([smooth[i : i + w // 3].mean() - smooth[max(0, i - w) : i].mean() if i >= w else 0 for i in range(n)])
drops = []
for i in np.argsort(-jump):
    if jump[i] <= 0: break
    if all(abs(t[i] - d) > 3 for d in drops): drops.append(float(t[i]))
    if len(drops) == 6: break
out = dict(duration_s=round(len(x) / SR, 3), beat_s=round(float(beat), 4), bpm=round(60 / float(beat), 1), beat_offset_s=round(offset, 3),
           drops_s=sorted(round(d, 2) for d in drops), rms_db=round(float(20 * np.log10(rms.max() + 1e-9)), 1))
# Energy by second, to see the structure.
out["low_by_second"] = [round(float(low[int(s / (hop / SR)) : int((s + 1) / (hop / SR))].mean()), 1) for s in range(int(out["duration_s"]))]
out["rms_by_second"] = [round(float(rms[int(s / (hop / SR)) : int((s + 1) / (hop / SR))].mean()), 3) for s in range(int(out["duration_s"]))]
print(json.dumps(out))
