#!/usr/bin/env python3
"""Zafe's payment sounds, synthesized from scratch (no samples, so no licences).

Payments are a signature, not UI feedback. Apple Pay is two clean strikes up an octave;
Mastercard's checkout sound is a 6-note, 1.3 s cut of its melody; Visa's is under a
second, tested for trust. Zafe's motif is three strikes on open intervals (E6, B6, E7:
a fifth, then a fourth). No thirds: they read as happy and game-like ("too frivolous",
as Apple put it about a candidate). Each approval is one strike of the motif; the
payment going out plays all of it. The timbre is struck metal with a coin's tick.

Rules from the research (docs/sounds.md): everything above 300 Hz (phone speakers),
short tails, the haptic taps in the same tempo as the strikes.

Usage: sounds.py <out_dir>   writes pay_<moment>.wav (44.1 kHz, 16-bit mono).
`scripts/sounds/build.sh` turns them into the app's Ogg files.
"""

import math
import random
import struct
import sys
import wave
from pathlib import Path

SR = 44_100
random.seed(7)  # deterministic output

def silence(seconds):
    return [0.0] * int(seconds * SR)


def mix(dst, src, at=0.0, gain=1.0):
    """Adds `src` into `dst` starting at `at` seconds, growing `dst` if needed."""
    start = int(at * SR)
    end = start + len(src)
    if end > len(dst):
        dst.extend([0.0] * (end - len(dst)))
    for i, v in enumerate(src):
        dst[start + i] += v * gain
    return dst


def tone(freq, partials, length, attack=0.003):
    """Modal synthesis: `partials` = [(ratio, amplitude, decay seconds)]."""
    n = int(length * SR)
    out = [0.0] * n
    a = max(1, int(attack * SR))
    for ratio, amp, tau in partials:
        f = freq * ratio
        if f > SR / 2.2:
            continue
        w = 2 * math.pi * f / SR
        phase = random.random() * 0.2
        k = math.exp(-1 / (tau * SR))
        env = amp
        for i in range(n):
            ramp = i / a if i < a else 1.0
            out[i] += math.sin(w * i + phase) * env * ramp
            env *= k
    # Release over the last quarter, so a note never stops with a click.
    r = n // 4
    for i in range(r):
        out[n - 1 - i] *= 0.5 - 0.5 * math.cos(math.pi * i / r)
    return out


# Instruments (partial ratios from the physics of each body).


def resonate(signal, freq, q):
    """Two-pole resonant band-pass."""
    r = math.exp(-math.pi * freq / (q * SR))
    c1 = 2 * r * math.cos(2 * math.pi * freq / SR)
    c2 = -r * r
    gain = 1 - r
    y1 = y2 = 0.0
    out = []
    for x in signal:
        y = gain * x + c1 * y1 + c2 * y2
        out.append(y)
        y2, y1 = y1, y
    return out


def noise(seconds, decay):
    n = int(seconds * SR)
    k = math.exp(-1 / (decay * SR))
    env, out = 1.0, []
    for _ in range(n):
        out.append((random.random() * 2 - 1) * env)
        env *= k
    return out


def reverb(signal, wet=0.18, tail=1.0):
    """Small room: four damped combs into two all-passes."""
    src = signal + [0.0] * int(tail * SR)
    combs = [(1557, 0.80), (1617, 0.79), (1491, 0.78), (1422, 0.77)]
    acc = [0.0] * len(src)
    for delay, fb in combs:
        buf = [0.0] * delay
        lp, idx = 0.0, 0
        for i, x in enumerate(src):
            y = buf[idx]
            lp = y * 0.7 + lp * 0.3
            buf[idx] = x + lp * fb
            idx = (idx + 1) % delay
            acc[i] += y
    for delay, g in [(556, 0.5), (225, 0.5)]:
        buf = [0.0] * delay
        idx = 0
        for i, x in enumerate(acc):
            b = buf[idx]
            y = -x * g + b
            buf[idx] = x + b * g
            idx = (idx + 1) % delay
            acc[i] = y
    return [d + w * wet / 4 for d, w in zip(src, acc)]


def finish(signal, peak_db=-1.0, fade=0.08):
    # Trim trailing near-silence, fade out, normalize.
    peak = max(abs(v) for v in signal) or 1.0
    end = len(signal)
    while end > 0 and abs(signal[end - 1]) < peak * 0.0015:
        end -= 1
    signal = signal[: min(len(signal), end + int(0.02 * SR))]
    f = int(fade * SR)
    for i in range(min(f, len(signal))):
        signal[-1 - i] *= i / f
    target = 10 ** (peak_db / 20)
    return [v / peak * target for v in signal]


def write(path, signal, gain_db=0.0):
    g = 10 ** (gain_db / 20)
    with wave.open(str(path), "wb") as w:
        w.setnchannels(1)
        w.setsampwidth(2)
        w.setframerate(SR)
        w.writeframes(b"".join(struct.pack("<h", int(max(-1, min(1, v * g)) * 32767)) for v in signal))


def note(name):
    names = {"C": -9, "C#": -8, "D": -7, "D#": -6, "E": -5, "F": -4, "F#": -3, "G": -2, "G#": -1, "A": 0, "A#": 1, "B": 2}
    pitch, octave = name[:-1], int(name[-1])
    return 440 * 2 ** ((names[pitch] + 12 * (octave - 4)) / 12)


# --- the payment signature ---

MOTIF = ["E6", "B6", "E7"]


def strike(f, ring=0.55, gain=1.0, damp=1.0):
    """A clean metal strike: a pure, slightly chorused fundamental, short inharmonic
    partials for the metal, and a coin's tick on top. `damp` < 1 mutes it."""
    s = silence(ring + 0.1)
    for detune in (-1.2, 1.2):  # a slow beat, like a real bell
        mix(s, tone(f + detune, [(1, 1.0, ring * damp), (2.0, 0.12, ring * 0.35 * damp)], ring + 0.1, 0.0015), 0.0, 0.5)
    mix(s, tone(f, [(2.92, 0.22, 0.05 * damp), (5.1, 0.12, 0.025 * damp), (8.3, 0.06, 0.012)], 0.2, 0.0008), 0.0, 1.0)
    mix(s, resonate(noise(0.01, 0.0008), 7000, 3), 0.0, 1.2 * damp)
    return [v * gain for v in s]


def seal(f, length=0.35):
    """Weight under the first strike, kept above 300 Hz so a phone can play it."""
    return tone(f, [(1, 1.0, 0.09), (2, 0.35, 0.05)], length, 0.004)


def coin_shimmer(at_f, length=0.6, count=7):
    """A few tiny high pings: small change settling."""
    s = silence(length)
    rnd = random.Random(3)
    for i in range(count):
        f = at_f * rnd.choice([2, 2.5, 3, 4]) * (1 + rnd.uniform(-0.01, 0.01))
        mix(s, tone(f, [(1, 1.0, 0.06)], 0.15, 0.001), 0.03 + i * 0.045 + rnd.uniform(0, 0.02), 0.1 * (1 - i / count))
    return s


def pay_approve():
    s = silence(0.05)
    mix(s, strike(note(MOTIF[0]), 0.5), 0.0, 0.9)
    return reverb(s, 0.1, 0.3)


def pay_ready():
    s = silence(0.05)
    mix(s, strike(note(MOTIF[0]), 0.45), 0.0, 0.8)
    mix(s, strike(note(MOTIF[1]), 0.6), 0.12, 0.9)
    return reverb(s, 0.12, 0.35)


def pay_sent():
    s = silence(0.05)
    mix(s, seal(note("E4")), 0.0, 0.5)
    mix(s, strike(note(MOTIF[0]), 0.45), 0.0, 0.8)
    mix(s, strike(note(MOTIF[1]), 0.5), 0.13, 0.85)
    mix(s, strike(note(MOTIF[2]), 0.9), 0.26, 1.0)
    mix(s, strike(note(MOTIF[0]) * 2 * 1.001, 0.9), 0.262, 0.35)  # octave doubling: the motif closes
    mix(s, coin_shimmer(note(MOTIF[2]) / 2), 0.32, 1.0)
    return reverb(s, 0.16, 0.5)


def pay_received():
    """Money arriving: the motif's top two notes, landing, with change settling."""
    s = silence(0.05)
    mix(s, strike(note(MOTIF[1]), 0.45), 0.0, 0.8)
    mix(s, strike(note(MOTIF[2]), 0.8), 0.11, 1.0)
    mix(s, coin_shimmer(note(MOTIF[2]) / 2, 0.5, 5), 0.16, 1.0)
    return reverb(s, 0.14, 0.4)


def pay_failed():
    """Not done: the same metal, muted, twice. Serious, not a cartoon 'wah-wah'."""
    s = silence(0.05)
    mix(s, seal(note("E4"), 0.2), 0.0, 0.4)
    mix(s, strike(note(MOTIF[0]) / 2, 0.3, 0.8, damp=0.25), 0.0, 1.0)
    mix(s, strike(note(MOTIF[0]) / 2, 0.3, 0.6, damp=0.25), 0.12, 1.0)
    return reverb(s, 0.06, 0.15)


SOUNDS_V4 = {
    "pay_approve": (pay_approve, -5),
    "pay_ready": (pay_ready, -3),
    "pay_sent": (pay_sent, 0),
    "pay_received": (pay_received, -1),
    "pay_failed": (pay_failed, -7),
}

SOUNDS = {
    "pay_approve": (pay_approve, -5),
    "pay_ready": (pay_ready, -3),
    "pay_sent": (pay_sent, 0),
    "pay_received": (pay_received, -1),
    "pay_failed": (pay_failed, -7),
}

if __name__ == "__main__":
    out = Path(sys.argv[1] if len(sys.argv) > 1 else "build/sounds")
    out.mkdir(parents=True, exist_ok=True)
    for name, (make, gain) in SOUNDS.items():
        signal = finish(make())
        write(out / f"{name}.wav", signal, gain)
        print(f"{name}.wav  {len(signal) / SR:.2f} s")
