#!/usr/bin/env python3
"""Run inside run_isolated_x11.sh: capture the published native app dictating synthetic speech.

See README.md in this folder for the environment variables and the invocation.
Click coordinates assume a 2200x1600 display at GDK_SCALE=2 (the window opens at 0,0, 2120x1560)."""
import json, os, shutil, signal, sqlite3, subprocess, sys, time
from pathlib import Path

SESSION = Path(os.environ["OFFSCREEN_SESSION_ROOT"])
HERE = Path(__file__).resolve().parent
BUNDLE = Path(os.environ["MLUVA_BUNDLE"])
ASSETS = Path(os.environ["MLUVA_QWEN_ASSETS"])
PCM = Path(os.environ["MLUVA_CAPTURE_PCM"])
OUT = Path(os.environ.get("HIGHLIGHT_OUT", SESSION / "out")); OUT.mkdir(parents=True, exist_ok=True)
SIZE = os.environ.get("HIGHLIGHT_SIZE", "2200x1600")
root = SESSION / "app"
for name in ("home", "config/mluva", "config/gtk-4.0", "data/mluva", "cache", "runtime", "state", "tmp"):
    (root / name).mkdir(mode=0o700, parents=True, exist_ok=True)
fixtures = HERE.parents[2] / "rust/mluva-gtk/tests/fixtures/released-bootstrap.json"
base = next(case["config"] for case in json.loads(fixtures.read_text())["cases"] if case["name"] == "normal-hide-reopen")
cfg = base | dict(transcription_provider="local", local_model="qwen3-1.7b", local_device="cpu",
                  language_code="eng", rewrite_provider="none", automatic_titles=False,
                  auto_copy_dictation=False, auto_copy_rewrite=False, auto_paste=False,
                  audio_retention_policy="never", welcome_completed=True)
REWRITE = os.environ.get("HIGHLIGHT_REWRITE") == "1"
POLISHED = os.environ.get("HIGHLIGHT_POLISHED", "")
if REWRITE:
    cfg.update(rewrite_provider="litellm", litellm_base_url="http://127.0.0.1:18080/v1", litellm_model="demo-rewrite", litellm_api_key_env="HIGHLIGHT_KEY")
cfg.update(json.loads(os.environ.get("HIGHLIGHT_CONFIG", "{}")))
(root / "config/mluva/config.json").write_text(json.dumps(cfg))
(root / "config/gtk-4.0/settings.ini").write_text("[Settings]\ngtk-cursor-blink=false\n")
theme = root / "state/omarchy/current/theme"; theme.mkdir(parents=True)
shutil.copy(os.environ.get("HIGHLIGHT_THEME", "/usr/share/omarchy/themes/vantablack/colors.toml"), theme / "colors.toml")
shutil.copytree(ASSETS / "models", root / "data/mluva/models", copy_function=os.link, symlinks=True)
shutil.copytree(ASSETS / "xdg-data/mluva/qwen-runtime", root / "data/mluva/qwen-runtime", copy_function=os.link, symlinks=True)
env = os.environ | dict(HOME=str(root / "home"), XDG_CONFIG_HOME=str(root / "config"), XDG_DATA_HOME=str(root / "data"),
    XDG_CACHE_HOME=str(root / "cache"), XDG_STATE_HOME=str(root / "state"), TMPDIR=str(root / "tmp"),
    PATH=f"{HERE / 'tools'}:{os.environ['PATH']}", MLUVA_CAPTURE_EVIDENCE=str(OUT), MLUVA_DISABLE_GLOBAL_SHORTCUT="1",
    GDK_BACKEND="x11", GDK_SCALE=os.environ.get("HIGHLIGHT_SCALE", "2"), GSK_RENDERER="cairo", HIGHLIGHT_KEY="synthetic",
    LANG="en_US.UTF-8", TZ="UTC")
env.pop("WAYLAND_DISPLAY", None)
assert not os.environ.get("WAYLAND_DISPLAY")

def gdbus(*args):
    return subprocess.run(["gdbus", "call", "--session", "--dest", "com.mluva.Linux", "--object-path", "/com/mluva/Linux",
                           "--method", "org.gtk.Actions.Activate", *args], capture_output=True, text=True)
def act(name): r = gdbus(name, "[]", "{}"); print("act", name, r.returncode, r.stderr.strip(), flush=True)
def wait(pred, what, timeout=60):
    end = time.monotonic() + timeout
    while time.monotonic() < end:
        v = pred()
        if v: return v
        time.sleep(0.05)
    raise TimeoutError(what)

command = [str(BUNDLE / "bin/mluva")]
if os.environ.get("HIGHLIGHT_NETNS", "1") == "1":
    peer = f'{sys.executable} {HERE / "fake_rewrite.py"} 18080 "$POLISHED" &' if REWRITE else ""
    env["POLISHED"] = POLISHED
    command = ["unshare", "-rn", "sh", "-c", f'ip link set lo up 2>/dev/null || true; {peer} exec "$@"', "sh", *command]
log = (OUT / "app.log").open("wb")
app = subprocess.Popen(command, env=env, cwd=root, stdout=log, stderr=log, start_new_session=True)
grab = None
try:
    wait(lambda: subprocess.run(["gdbus", "call", "--session", "--dest", "org.freedesktop.DBus", "--object-path", "/org/freedesktop/DBus",
        "--method", "org.freedesktop.DBus.GetNameOwner", "com.mluva.Linux"], capture_output=True).returncode == 0, "app name", 30)
    time.sleep(2.0)
    print(subprocess.run([sys.executable, str(HERE / "xinput.py"), "focus"], capture_output=True, text=True, env=os.environ), flush=True)
    time.sleep(1.0)
    subprocess.run(["import", "-window", "root", str(OUT / "idle.png")], check=True)
    grab = subprocess.Popen(["ffmpeg", "-y", "-loglevel", "error", "-f", "x11grab", "-framerate", os.environ.get("HIGHLIGHT_FPS", "30"),
        "-video_size", SIZE, "-draw_mouse", "0", "-i", os.environ["DISPLAY"], "-c:v", "libx264", "-preset", "ultrafast", "-crf", "0",
        "-pix_fmt", "yuv444p", str(OUT / "take.mkv")], stdin=subprocess.PIPE)
    grab_started = time.time(); time.sleep(1.5)
    timeline = {}
    t0 = time.time(); timeline["grab_to_record_s"] = None; act("record"); timeline["record"] = time.time()
    wait(lambda: (OUT / "input-finished").exists(), "input finished", 40)
    timeline["input_finished"] = time.time()
    time.sleep(2.0)
    act("record"); timeline["stop"] = time.time()
    def history():
        for p in (root / "data/mluva").glob("*.sqlite*"):
            try:
                with sqlite3.connect(f"file:{p}?mode=ro", uri=True) as s:
                    if s.execute("select 1 from sqlite_master where name='transcription_history'").fetchone():
                        rows = s.execute("select raw_text from transcription_history").fetchall()
                        if rows: return rows
            except sqlite3.Error: pass
    rows = wait(history, "history row", 60)
    timeline["history_row"] = time.time()
    print("raw_text:", rows[0][0], flush=True)
    time.sleep(3.0)
    if REWRITE:
        r = subprocess.run([sys.executable, str(HERE / "xinput.py"), "click", *os.environ.get("HIGHLIGHT_POLISH_XY", "114,1156").split(",")], capture_output=True, text=True, env=os.environ)
        print(r.stdout.strip(), r.stderr.strip()[-300:], flush=True); timeline["polish"] = time.time()
        time.sleep(0.8); subprocess.run([sys.executable, str(HERE / "xinput.py"), "move", "2190", "1590"], env=os.environ)
        time.sleep(7.0); subprocess.run(["import", "-window", "root", str(OUT / "polished.png")], check=True)
        time.sleep(2.0)
    for action in os.environ.get("HIGHLIGHT_SCENES", "").split(","):
        if action:
            act(action); timeline[action] = time.time(); time.sleep(3.0)
            if action == "settings":
                subprocess.run([sys.executable, str(HERE / "xinput.py"), "click", *os.environ.get("HIGHLIGHT_PROVIDERS_XY", "802,269").split(",")], env=os.environ); timeline["providers"] = time.time(); time.sleep(3.0)
            subprocess.run(["import", "-window", "root", str(OUT / f"scene-{action}.png")], check=True)
    subprocess.run(["import", "-window", "root", str(OUT / "final.png")], check=True)
    timeline["grab_start"] = grab_started
    (OUT / "timeline.json").write_text(json.dumps(timeline))
    grab.stdin.write(b"q"); grab.stdin.flush(); grab.wait(timeout=30); grab = None
    act("quit"); app.wait(timeout=15)
finally:
    if grab and grab.poll() is None:
        grab.send_signal(signal.SIGINT); grab.wait(timeout=30)
    if app.poll() is None:
        os.killpg(app.pid, signal.SIGTERM)
        try: app.wait(timeout=8)
        except subprocess.TimeoutExpired: os.killpg(app.pid, signal.SIGKILL)
print("done", OUT)
