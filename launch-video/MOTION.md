# Motion and voice decisions

Daniel's delivery sets the timing. The 83.27-second camera recording becomes 72.37 seconds after ten long internal pauses and unused lead/tail space are shortened. All 194 spoken tokens remain. Picture and sound use identical 30 fps source boundaries, with 6 ms audio fades at the pause cuts. Speech speed and pitch stay unchanged.

The original Figma feature order guides 34 native-footage intervals: setup, F9 dictation/paste, the tiled and floating recorder, desktop switching, Commands, history, themes, Live rewrite, task questions and closing. Stable holds absorb timing changes; window transitions keep explicit anchors. [The edit record](reference/edit.json) preserves these mappings.

The desktop occupies 1728×1080 at (96, 0). Welcome uses a 1.24× view; app details use 1.42×, with cubic Bézier `(0.42, 0, 0.2, 1)` and 0.7-second ramps around `(60%, 46.3%)`. The recorder shows the full desktop. The 260×349 presenter sits at x=1544, bottom=-14; during the recorder/desktop-switch section, x=116 keeps the pinned widget visible. His name appears from 1.05 to 8.9 seconds. Product information fades out by 9.1 seconds, and closing copy begins at 63.43 seconds.

[Robust Video Matting](https://github.com/PeterL1n/RobustVideoMatting) separates the recorded presenter, preserving temporal state. A 432×580, 30 fps VP9 alpha video retains the recorded face and movement. The composition uses its alpha directly, with no backdrop, outline, shadow or added edge fades.

[ElevenLabs Scribe v2](https://elevenlabs.io/docs/api-reference/speech-to-text/convert) supplies word timing. Forty-eight short phrases at the top leave lower dictation controls visible. Adwaita Sans at 58 px/800 weight uses white text on a dark backing; selected spoken words gain coral emphasis and a 180 ms pulse up to 1.055×. Each phrase enters from 0.98× to 1× over 120 ms. Captions normalize Omarchy/Live spelling and one text-model recognition error. Raw transcript and caption text remain in the private archive.

[DeepFilterNet3](https://github.com/Rikorose/DeepFilterNet) cleans the original audio locally, with noise attenuation limited to 18 dB. The final voice treatment uses a 65 Hz high-pass, +1.6 dB around 145 Hz, −2.2 dB around 340 Hz, +2.1 dB around 2.9 kHz, a +1.1 dB high shelf above 6.5 kHz, light de-essing and moderate 2.6:1 compression. Voice loudness is −18.12 LUFS; the music bed remains about 21.5 dB below it across spoken intervals.

The finished mix measures −16.01 LUFS and −3.61 dBTP. Music fades in over 1.2 seconds and out over 1.4 seconds. The denoiser's compensated output is aligned with the matched source cuts; 30 ms of tail silence restores the exact sample count. Five waveform comparisons find less than 0.1 ms offset. A fresh transcription of the encoded mix matches the previous normalized wording.

The approved 48 kHz stereo mix is now a single render input. Render picture muted, then attach the mix with `script/master.py`; do not apply the old 2,048-sample Remotion correction or normalize the mix again. Review final encoded picture, caption bounds, full decoding and audio alignment when changing the composition or processing.
