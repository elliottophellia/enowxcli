import sys, os, numpy as np, wave, struct
sys.path.insert(0, '/Volumes/SSD/.ctmp/demo')
import schedule as S

SR = 44100
N = int(S.SECONDS * SR)
buf = np.zeros(N, dtype=np.float32)

def place(sig, t):
    i = int(t * SR)
    j = min(i + len(sig), N)
    if i < N:
        buf[i:j] += sig[:j - i]

def env(n, a, d):
    e = np.ones(n)
    ai, di = int(a * SR), int(d * SR)
    if ai: e[:ai] = np.linspace(0, 1, ai)
    if di: e[-di:] = np.linspace(1, 0, di) * e[-di:]
    return e

def click(vol=0.18):                       # key press: short noise burst
    n = int(0.03 * SR)
    s = np.random.randn(n) * env(n, 0.001, 0.028)
    return s * vol

def whoosh(dur=0.5, vol=0.16):             # delegation fan-out
    n = int(dur * SR); t = np.arange(n) / SR
    noise = np.random.randn(n)
    # simple one-pole lowpass sweeping up
    out = np.zeros(n); a = 0.0
    for k in range(n):
        cutoff = 0.02 + 0.25 * (k / n)
        a += cutoff * (noise[k] - a)
        out[k] = a
    return out * env(n, 0.08, 0.3) * vol

def ding(vol=0.22):                         # report done: bell
    n = int(0.9 * SR); t = np.arange(n) / SR
    s = sum(np.sin(2*np.pi*f*t) * g for f, g in [(880,1),(1320,0.5),(1760,0.25)])
    return s * np.exp(-t * 4) * vol

def pop(vol=0.12):                          # row appears
    n = int(0.12 * SR); t = np.arange(n) / SR
    f = np.linspace(620, 300, n)
    s = np.sin(2*np.pi*np.cumsum(f)/SR)
    return s * np.exp(-t * 18) * vol

# --- events, from the shared schedule -----------------------------------
# Key clicks while typing the prompt.
for k in range(len(S.PROMPT)):
    place(click(), S.TYPE_START + k / S.TYPE_CPS)
# Send whoosh.
place(whoosh(0.4, 0.14), S.SEND)
# A pop as each agent row appears, a soft whoosh for the build pair.
for (agent, task, model, start, done) in S.AGENTS:
    place(pop(), start)
# Ding when the report lands.
place(ding(), S.REPORT_AT)

# --- ambient music: Am–F–C–G pad, very soft -----------------------------
def note(freq, start, dur, vol):
    n = int(dur * SR); t = np.arange(n) / SR
    s = (np.sin(2*np.pi*freq*t) + 0.5*np.sin(2*np.pi*freq*1.003*t)) * 0.5
    place(s * env(n, 0.4, 0.6) * vol, start)
chords = {"A":220.0,"C":261.63,"E":329.63,"F":349.23,"G":392.0}
prog = [("A","C","E"), ("F","A","C"), ("C","E","G"), ("G","C","E")]
bar = S.SECONDS / 4
for bi, ch in enumerate(prog):
    for name in ch:
        note(chords[name]/2, bi*bar, bar, 0.05)

# Master fade, soft-clip, normalise.
master = env(N, 0.6, 1.0)
out = np.tanh(buf * master * 1.4)
out = out / (np.max(np.abs(out)) + 1e-9) * 0.9
pcm = (out * 32767).astype(np.int16)

with wave.open("/Volumes/SSD/.ctmp/demo/audio.wav", "w") as w:
    w.setnchannels(1); w.setsampwidth(2); w.setframerate(SR)
    w.writeframes(pcm.tobytes())
print("audio.wav written")
