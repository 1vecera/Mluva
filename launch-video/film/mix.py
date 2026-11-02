# /// script
# requires-python = ">=3.12"
# dependencies = ["numpy","soundfile"]
# ///
"""Mix voice and music: music sits about 21.5 dB under the voice across speech, as in v7, then the master goes to -16 LUFS, -1.8 dBTP.

    mix.py VOICE.wav MUSIC.wav PLAN.json OUT.wav
"""
import json, re, subprocess, sys
import numpy as np, soundfile as sf

voice_path, music_path, plan_path, out = sys.argv[1:5]
voice, sr = sf.read(voice_path, dtype="float64")
music, _ = sf.read(music_path, dtype="float64")
plan = json.load(open(plan_path))
n = len(voice)
music = music[:n] if len(music) >= n else np.pad(music, ((0, n - len(music)), (0, 0)))
speech = np.zeros(n, bool)
for phrase in plan["poppy_captions"]:
    for word in phrase["words"]:
        speech[int(word["start"] * sr) : int(word["end"] * sr)] = True
rms = lambda x: np.sqrt(np.mean(x**2))
gain = rms(voice[speech]) / rms(music[speech]) * 10 ** (-21.5 / 20)
env = np.ones(n)
fade_in, fade_out = int(1.2 * sr), int(1.4 * sr)
env[:fade_in] = np.linspace(0, 1, fade_in)
env[-fade_out:] = np.linspace(1, 0, fade_out)
bed = music * gain * env[:, None]
mix = np.stack([voice, voice], 1) + bed
sf.write(out + ".pre.wav", mix, sr, subtype="FLOAT")
options = "I=-16:TP=-1.8:LRA=8"
first = subprocess.run(["ffmpeg", "-nostdin", "-hide_banner", "-i", out + ".pre.wav", "-af", f"loudnorm={options}:print_format=json", "-f", "null", "-"], capture_output=True)
m = json.loads(re.search(rb'\{\s*"input_i".*?\}', first.stderr, re.S)[0])
corr = ":".join(f"{a}={m[b]}" for a, b in [("measured_I", "input_i"), ("measured_TP", "input_tp"), ("measured_LRA", "input_lra"), ("measured_thresh", "input_thresh"), ("offset", "target_offset")])
subprocess.run(["ffmpeg", "-nostdin", "-v", "error", "-y", "-i", out + ".pre.wav", "-af", f"loudnorm={options}:{corr}:linear=true,aresample=48000,apad=whole_len={n},atrim=end_sample={n}", "-c:a", "pcm_s24le", out], check=True)
print(json.dumps({"music_below_voice_db": 21.5, "music_gain": gain, "samples": n}))
