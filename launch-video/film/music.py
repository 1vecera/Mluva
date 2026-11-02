# /// script
# requires-python = ">=3.12"
# dependencies = ["numpy","scipy","soundfile"]
# ///
"""A restrained music bed made from nothing but arithmetic, so its rights are not in question: warm chords, water-like plucks, a long room.

    music.py OUT.wav SECONDS

The harmony moves at the film's section changes, and the plucks thin out under the dropped-features passage.
"""
import sys
import numpy as np, soundfile as sf
from scipy.signal import butter, fftconvolve, sosfilt

SR = 48000
out, seconds = sys.argv[1], float(sys.argv[2])
N = int(seconds * SR)
rng = np.random.default_rng(719)
t = np.arange(N) / SR


def note(name):
    table = {"C": 0, "C#": 1, "D": 2, "D#": 3, "E": 4, "F": 5, "F#": 6, "G": 7, "G#": 8, "A": 9, "A#": 10, "B": 11}
    octave = int(name[-1])
    return 440.0 * 2 ** ((table[name[:-1]] + 12 * (octave + 1) - 69) / 12)


# (film seconds, chord voicing) on the film's own sections.
SECTIONS = [
    (0.0, ["D3", "A3", "E4", "F#4"]),
    (5.4, ["B2", "F#3", "D4", "A4"]),
    (11.7, ["G2", "D3", "B3", "F#4"]),
    (15.8, ["D3", "A3", "F#4", "B4"]),
    (20.7, ["B2", "A3", "D4", "F#4"]),
    (24.2, ["G2", "B3", "D4", "A4"]),
    (29.1, ["E3", "B3", "D4", "G4"]),
    (34.0, ["A2", "E3", "C#4", "G4"]),
    (40.3, ["F#2", "C#3", "A3", "E4"]),
    (46.2, ["G2", "D3", "A3", "B3"]),
    (49.9, ["D3", "A3", "D4", "F#4"]),
    (54.8, ["B2", "A3", "E4", "F#4"]),
    (59.3, ["G2", "D3", "B3", "E4"]),
    (63.7, ["A2", "E3", "B3", "F#4"]),
    (66.7, ["D3", "A3", "E4", "F#4"]),
]
pad = np.zeros((N, 2))
for i, (start, chord) in enumerate(SECTIONS):
    end = SECTIONS[i + 1][0] if i + 1 < len(SECTIONS) else seconds
    a, b = int(max(0, start - 0.5) * SR), min(N, int((end + 1.8) * SR))
    env = np.ones(b - a)
    rise, fall = int(1.0 * SR), int(2.0 * SR)
    env[:rise] = np.sin(np.linspace(0, np.pi / 2, rise)) ** 2
    env[-fall:] *= np.cos(np.linspace(0, np.pi / 2, fall)) ** 2
    tt = t[a:b]
    for name in chord:
        f = note(name)
        for detune, side in ((-0.09, 0), (0.09, 1), (0.0, 0), (0.03, 1)):
            for harmonic, gain in ((1, 1.0), (2, 0.28), (3, 0.09), (4, 0.03)):
                phase = rng.uniform(0, 2 * np.pi)
                vibrato = 0.0015 * np.sin(2 * np.pi * rng.uniform(0.09, 0.22) * tt + phase)
                pad[a:b, side] += gain * env * np.sin(2 * np.pi * f * (harmonic + detune / f) * tt * (1 + vibrato) + phase)
pad /= np.abs(pad).max()

# Plucks: soft sine drops from the chord tones, sparse and uneven like water.
pluck = np.zeros((N, 2))
for i, (start, chord) in enumerate(SECTIONS):
    end = SECTIONS[i + 1][0] if i + 1 < len(SECTIONS) else seconds
    if 40.3 <= start < 49.9:
        continue
    at = start + 0.15
    while at < end - 0.3:
        name = chord[rng.integers(1, len(chord))]
        f = note(name) * (2 if rng.random() < 0.55 else 4)
        length = int(2.4 * SR)
        a = int(at * SR)
        if a + length >= N:
            break
        tt = np.arange(length) / SR
        drop = np.exp(-tt * 3.4) * (np.sin(2 * np.pi * f * tt) + 0.18 * np.exp(-tt * 9) * np.sin(2 * np.pi * 2 * f * tt))
        drop[:96] *= np.linspace(0, 1, 96)
        pan = rng.uniform(0.25, 0.75)
        gain = rng.uniform(0.55, 1.0)
        pluck[a : a + length, 0] += gain * (1 - pan) * drop
        pluck[a : a + length, 1] += gain * pan * drop
        at += rng.choice([0.84, 1.26, 1.68, 2.1])
pluck /= np.abs(pluck).max()

# A long, dark synthetic room.
ir_n = int(3.2 * SR)
decay = np.exp(-np.arange(ir_n) / SR * 2.1)
lp = butter(2, 3800, "low", fs=SR, output="sos")
room = np.stack([sosfilt(lp, rng.standard_normal(ir_n)) * decay for _ in range(2)], axis=1)
room /= np.sqrt((room**2).sum(axis=0))
dry = 0.72 * pad + 0.42 * pluck
wet = np.stack([fftconvolve(dry[:, c], room[:, c])[:N] for c in range(2)], axis=1)
mix = 0.55 * dry + 0.9 * wet
hp = butter(2, 70, "high", fs=SR, output="sos")
lp2 = butter(2, 5200, "low", fs=SR, output="sos")
mix = sosfilt(lp2, sosfilt(hp, mix, axis=0), axis=0)
mix /= np.abs(mix).max() / 0.7
sf.write(out, mix, SR, subtype="PCM_24")
print("written", out, len(mix) / SR)
