#!/usr/bin/env python3
"""Measure live recordings from scripts/live/listen.sh.

  measure.py stats FILE...            length, sound span, peak, rms
  measure.py match REC REF [FROM TO]  find REF[FROM:TO s] in REC: offset, correlation, level
  measure.py tempo FILE [FROM]        beat period from the envelope -> BPM
  measure.py band ON OFF LO HI        energy difference of a band, dB
"""
import subprocess, sys
import numpy as np

def load(p):
    raw = subprocess.run(["ffmpeg", "-v", "error", "-i", p, "-f", "f32le", "-ac", "1", "-ar", "48000", "-"], capture_output=True).stdout
    return np.frombuffer(raw, dtype=np.float32)

def stats(p):
    x = load(p); nz = np.nonzero(np.abs(x) > 1e-4)[0]
    span = (nz[-1] - nz[0]) / 48000 if len(nz) else 0.0
    print(f"{p}: {len(x)/48000:.2f} s, sound {span:.2f} s, peak {np.abs(x).max():.3f}, rms {np.sqrt((x**2).mean()):.4f}")

def match(rec, ref, a=0.0, b=None):
    x, r = load(rec), load(ref)
    seg = r[int(float(a) * 48000): int(float(b) * 48000) if b else None]
    n = 1 << int(np.ceil(np.log2(len(x) + len(seg))))
    xc = np.fft.irfft(np.fft.rfft(x, n) * np.conj(np.fft.rfft(seg, n)), n)[: len(x)]
    lag = int(np.argmax(xc)); got = x[lag: lag + len(seg)]; seg = seg[: len(got)]
    c = float(np.dot(got, seg) / np.sqrt(np.dot(got, got) * np.dot(seg, seg) + 1e-20))
    print(f"found at {lag/48000:.3f} s, correlation {c:.4f}, level x{np.sqrt((got**2).mean()/((seg**2).mean()+1e-20)):.3f}")

def tempo(p, start=2.0):
    x = load(p)[int(float(start) * 48000):]; x = x[: len(x) // 480 * 480]
    env = np.abs(x).reshape(-1, 480).max(1); env = env - env.mean()
    ac = np.correlate(env, env, "full")[len(env) - 1:]
    lo, hi = 30, min(90, len(ac) - 2)
    lag = lo + int(np.argmax(ac[lo:hi]))
    a, b, c = ac[lag - 1], ac[lag], ac[lag + 1]
    beat = (lag + 0.5 * (a - c) / (a - 2 * b + c)) / 100
    print(f"{p}: beat {beat:.3f} s -> {60/beat:.1f} BPM")

def band(on, off, lo, hi):
    def db(x):
        X = np.abs(np.fft.rfft(x)) ** 2; f = np.fft.rfftfreq(len(x), 1 / 48000)
        return 10 * np.log10(X[(f >= float(lo)) & (f < float(hi))].sum() + 1e-20)
    print(f"{lo}-{hi} Hz: {db(load(on)) - db(load(off)):+.1f} dB")

if __name__ == "__main__":
    cmd, *args = sys.argv[1:]
    if cmd == "stats":
        for p in args:
            stats(p)
    else:
        {"match": match, "tempo": tempo, "band": band}[cmd](*args)
