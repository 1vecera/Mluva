"""Create a quiet original synth pulse bed for the launch film; no sampled music."""

import argparse
import json
import subprocess
from pathlib import Path


def main() -> None:
    """Synthesize a restrained stereo bed with declared oscillators and no external source."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output", type=Path)
    parser.add_argument("--duration", type=float, default=55)
    args = parser.parse_args()
    if not 1 < args.duration < 60:
        raise ValueError("Use a duration between one and sixty seconds")
    args.output.parent.mkdir(parents=True, exist_ok=True)
    # A soft low pulse and a higher, shorter reply; gentle stereo detuning supplies space.
    bass = "0.022*sin(2*PI*65.406*t)*exp(-5*mod(t,0.96))"
    warm = "0.012*(sin(2*PI*130.813*t)+0.3*sin(2*PI*261.626*t))*exp(-8*mod(t,0.48))"
    bell = "0.005*sin(2*PI*392*t)*exp(-15*mod(t+0.24,0.96))"
    pad_l = "0.003*(sin(2*PI*155.563*t)+sin(2*PI*195.998*t))"
    pad_r = "0.003*(sin(2*PI*155.713*t)+sin(2*PI*196.148*t))"
    left, right = f"{bass}+{warm}+{bell}+{pad_l}", f"{bass}+{warm}+{bell}+{pad_r}"
    expression = f"aevalsrc='{left}|{right}':s=48000:d={args.duration}"
    filters = f"highpass=f=45,lowpass=f=2200,afade=t=in:d=0.8,afade=t=out:st={args.duration - 1.4}:d=1.4"
    subprocess.run(
        [
            "ffmpeg",
            "-nostdin",
            "-v",
            "error",
            "-y",
            "-f",
            "lavfi",
            "-i",
            expression,
            "-af",
            filters,
            "-c:a",
            "flac" if args.output.suffix == ".flac" else "pcm_s16le",
            str(args.output),
        ],
        check=True,
    )
    args.output.with_suffix(".recipe.json").write_text(
        json.dumps(
            {"description": __doc__, "duration": args.duration, "expression": expression, "filters": filters}, indent=2
        )
        + "\n"
    )


if __name__ == "__main__":
    main()
