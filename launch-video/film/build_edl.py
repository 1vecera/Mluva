# /// script
# requires-python = ">=3.12"
# dependencies = ["numpy","soundfile","opencv-python-headless"]
# ///
"""Choose matched picture/audio cut points: every boundary on a 30 fps frame, inside silence, at the
pair of frames whose look is closest. Writes the EDL and the word map onto film time."""
import json,sys
import numpy as np,soundfile as sf,cv2
ROOT="/home/vecera/Videos/Mluva/launch-2026-recording"
FPS=30
y,sr=sf.read(f"{ROOT}/work/audio/df/mono.wav")
hop=sr//100
env=np.array([np.sqrt(np.mean(y[i:i+hop]**2)) for i in range(0,len(y)-hop,hop)])
SIL=10**(-46/20)
def silent(t0,t1):
    a,b=int(t0*100),int(t1*100)+1
    return np.all(env[max(a,0):b]<SIL)
cap=cv2.VideoCapture(f"{ROOT}/IMG_6947.MOV")
frames={}
def frame(n):
    if n in frames: return frames[n]
    cap.set(cv2.CAP_PROP_POS_FRAMES,n); ok,f=cap.read(); assert ok,n
    f=cv2.resize(f,(216,384),interpolation=cv2.INTER_AREA)
    f=f[60:300] # face and shoulders
    frames[n]=cv2.GaussianBlur(f,(5,5),0).astype(np.float32); return frames[n]
words=[w for w in json.load(open(f"{ROOT}/work/audio/scribe/take.scribe.json"))['words'] if w['type']=='word']
# (out window, in window, seconds of pause left at the cut)  source seconds. None for keep-as-is.
#  out_from..out_to: where the kept audio may end. in_from..in_to: where it may resume.
CUTS=[
 ("Mluva. | It's simple",   (7.84,8.40),(8.30,8.84),0.46),
 ("talk, | and then",       (10.90,11.30),(11.20,11.60),0.42),
 ("appear | in your app",   (14.90,15.20),(15.20,15.50),0.20),
 ("app. | But what",        (16.08,16.60),(16.50,16.98),0.55),
 ("to, | you record",       (25.46,25.80),(25.70,26.10),0.44),
 ("again, | and the text",  (29.32,29.60),(29.50,29.80),0.34),
 ("sent. | [If--] You",     (34.32,34.90),(35.60,35.74),0.62),
 ("Shift, | highlight",     (38.72,38.90),(38.90,39.16),0.34),
 ("highlight, | [uh] what", (39.86,40.2),(40.70,40.71),0.16),
 ("know, | and",            (42.68,42.85),(42.90,43.10),0.36),
 ("included | on",          (45.42,45.70),(45.75,46.06),0.18),
 ("automatically. | In",    (48.00,48.90),(49.10,49.96),0.72),
 ("agent. | I've",          (58.96,59.30),(59.25,59.64),0.58),
 ("water. | You",           (65.30,66.20),(66.30,67.30),0.86),
 ("local | or",             (68.80,69.00),(68.95,69.20),0.30),
 ("model, | [and, uh] I",   (70.22,70.70),(72.30,72.74),0.46),
 ("know? | It's",           (76.88,77.15),(77.10,77.42),0.40),
 ("now. | Cheers",          (81.26,81.60),(81.55,81.90),0.48),
]
START=3.30; END=83.10
boundaries=[]
for name,(oa,ob),(ia,ib),left in CUTS:
    best=None
    for o in range(int(np.ceil(oa*FPS)),int(ob*FPS)+1):
        for i in range(int(np.ceil(ia*FPS)),int(ib*FPS)+1):
            kept_pause=None
            removed=(i-o)/FPS
            # nearest word edges
            before=max(w['end'] for w in words if w['end']<=o/FPS+0.09)
            after=min(w['start'] for w in words if w['start']>=i/FPS-0.09) if 'uh' not in name else min(w['start'] for w in words if w['start']>=i/FPS)
            pause=(o/FPS-before)+(after-i/FPS)
            if abs(pause-left)>0.1: continue
            if not silent(o/FPS-0.02,o/FPS+0.02) or (name!='highlight, | [uh] what' and not silent(i/FPS-0.02,i/FPS+0.02)): continue
            d=float(np.mean(np.abs(frame(o-1)-frame(i))))
            score=d+abs(pause-left)*6
            if best is None or score<best[0]: best=(score,o,i,d,pause)
    if best is None: raise SystemExit(f"no cut for {name}")
    _,o,i,d,pause=best
    boundaries.append(dict(name=name,out=o,inn=i,look=round(d,2),pause=round(pause,3)))
    print(f"{name:28} out {o/FPS:6.2f} in {i/FPS:6.2f} removed {(i-o)/FPS:5.2f} pause {pause:.2f} look-diff {d:5.2f}")
segs=[];cur=round(START*FPS)
for b in boundaries:
    segs.append([cur,b['out']]);cur=b['inn']
segs.append([cur,round(END*FPS)])
film=0;out=[]
for a,b in segs:
    out.append(dict(src_start=a,src_end=b,film_start=film));film+=b-a
def to_film(t):
    n=t*FPS
    for s in out:
        if s['src_start']<=n<=s['src_end']: return s['film_start']/FPS+(t-s['src_start']/FPS)
    return None
wm=[]
for w in words:
    a,b=to_film(w['start']),to_film(w['end'])
    wm.append(dict(text=w['text'],src_start=w['start'],src_end=w['end'],start=None if a is None else round(a,3),end=None if b is None else round(b,3)))
json.dump(dict(fps=FPS,segments=out,duration=film/FPS,cuts=boundaries,words=wm),open(f"{ROOT}/work/edit/edl.json","w"),indent=1,ensure_ascii=False)
print("film",film/FPS,"segments",len(out))
print(" ".join(f"{w['text']}@{w['start']}" for w in wm))
