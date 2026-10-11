#!/usr/bin/env python3
"""Synthesize the game's sound effects in the style of the Game Boy Advance.

GBA games like Advance Wars mostly played short *sampled* instruments (brass,
plucks, bells, drums, orchestral hits) through an 8-bit mixer at ~16-18 kHz,
rather than the pure square waves of older consoles. So the sounds here are
built from richer sources and then put through a GBA-style output stage:

- FM synthesis (two operators) for bells, chimes, coins and brass stabs
- Karplus-Strong plucked strings for wooden plucks and marimba-like ticks
- band-limited saws through a resonant low-pass filter for brass swells
- filtered noise for impacts, whooshes, footsteps and dust
- a pitch-dropping sine "kick" for thumps and booms
- output: sample-and-hold at 18157 Hz and 8-bit quantization (the GBA's
  Direct Sound mixer), plus a short room echo where it helps

`assets/data/audio.ron` maps each id to its file.

Usage:
  python3 tools/audio/sfx.py [OUT_DIR] [--play ID]   # default: assets/audio/sfx

Writes 16-bit mono WAV at 32 kHz. Standard library only (`--play` uses macOS
`afplay` to audition one sound).
"""

import math
import random
import struct
import subprocess
import sys
import wave
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
RATE = 32000
GBA_RATE = 18157  # a common GBA Direct Sound mixing rate
TAU = 2 * math.pi


def note(name):
    """'C5' -> frequency in Hz (A4 = 440)."""
    semis = {"C": -9, "D": -7, "E": -5, "F": -4, "G": -2, "A": 0, "B": 2}[name[0]]
    rest = name[1:]
    if rest[0] == "#":
        semis, rest = semis + 1, rest[1:]
    elif rest[0] == "b":
        semis, rest = semis - 1, rest[1:]
    return 440.0 * 2 ** ((semis + (int(rest) - 4) * 12) / 12)


def samples(length):
    return int(length * RATE)


def env_ad(i, n, attack, curve=2.0):
    """Attack then a curved decay over the rest of n samples."""
    a = max(1, int(attack * RATE))
    if i < a:
        return i / a
    return max(0.0, 1 - (i - a) / max(1, n - a)) ** curve


def place(sound, delay):
    return [0.0] * samples(delay) + sound


# --- Sources -----------------------------------------------------------------

def fm(freq, length, ratio=1.0, index=2.0, index_decay=1.0, volume=0.5, attack=0.002, curve=2.0, slide_to=None):
    """Two-operator FM: a sine carrier modulated by a sine at freq*ratio. The
    modulation index fades with the envelope, so tones start bright and
    mellow out, like a struck bell or a brass stab."""
    n = samples(length)
    out, pc, pm = [], 0.0, 0.0
    for i in range(n):
        f = freq if slide_to is None else freq + (slide_to - freq) * i / n
        e = env_ad(i, n, attack, curve)
        pm += TAU * f * ratio / RATE
        pc += TAU * f / RATE
        idx = index * (e ** index_decay)
        out.append(math.sin(pc + idx * math.sin(pm)) * e * volume)
    return out


def pluck(freq, length, volume=0.5, brightness=0.5, seed=1):
    """Karplus-Strong plucked string: a burst of noise ringing in a tuned
    delay line, softened a little each pass (brightness 0..1)."""
    rng = random.Random(seed)
    period = max(2, int(RATE / freq))
    buf = [rng.uniform(-1, 1) for _ in range(period)]
    n = samples(length)
    out = []
    damp = 0.5 + 0.49 * brightness
    for i in range(n):
        j = i % period
        nxt = buf[(j + 1) % period]
        s = buf[j]
        buf[j] = damp * s + (1 - damp) * nxt
        buf[j] *= 0.996
        out.append(s * volume * env_ad(i, n, 0.001, 1.0))
    return out


def brass(freqs, length, volume=0.4, attack=0.03, cutoff=(400, 3200), detune=0.004, resonance=0.25):
    """Detuned saws (one or a chord) through a resonant low-pass whose cutoff
    opens with the attack and closes as it decays: a synth-brass stab."""
    n = samples(length)
    phases = [[0.0, 0.0] for _ in freqs]
    raw = []
    for i in range(n):
        s = 0.0
        for k, f in enumerate(freqs):
            for d, sign in enumerate((1, -1)):
                phases[k][d] = (phases[k][d] + f * (1 + sign * detune) / RATE) % 1.0
                s += 2 * phases[k][d] - 1
        raw.append(s / (2 * len(freqs)))
    lo, hi = cutoff
    cut = [lo + (hi - lo) * env_ad(i, n, attack, 1.5) for i in range(n)]
    filtered = svf(raw, cut, resonance, "low")
    return [s * volume * env_ad(i, n, attack, 1.6) for i, s in enumerate(filtered)]


def noise(length, volume=0.5, attack=0.001, curve=2.0, seed=1):
    rng = random.Random(seed)
    n = samples(length)
    return [rng.uniform(-1, 1) * volume * env_ad(i, n, attack, curve) for i in range(n)]


def steady_noise(length, volume=0.5, seed=7):
    """Noise at a constant level, for loops."""
    rng = random.Random(seed)
    return [rng.uniform(-1, 1) * volume for _ in range(samples(length))]


def kick(length, start=150.0, end=45.0, volume=0.8, curve=2.5):
    """A sine whose pitch drops fast: the body of a thump or a boom."""
    n = samples(length)
    out, p = [], 0.0
    for i in range(n):
        t = i / n
        f = end + (start - end) * math.exp(-t * 9)
        p += TAU * f / RATE
        out.append(math.sin(p) * volume * env_ad(i, n, 0.001, curve))
    return out


# --- Filters and effects -----------------------------------------------------

def svf(signal, cutoff, resonance=0.2, mode="low"):
    """State-variable filter; cutoff is a number or a list per sample."""
    low = band = 0.0
    out = []
    q = 1.0 - resonance * 0.9
    for i, x in enumerate(signal):
        fc = cutoff[i] if isinstance(cutoff, list) else cutoff
        f = 2 * math.sin(math.pi * min(fc, RATE / 6) / RATE)
        low += f * band
        high = x - low - q * band
        band += f * high
        out.append({"low": low, "band": band, "high": high}[mode])
    return out


def sweep(start, end, length, curve=1.0):
    n = samples(length)
    return [start + (end - start) * (i / max(1, n - 1)) ** curve for i in range(n)]


def echo(signal, delay=0.09, feedback=0.3, mix_level=0.35, repeats=3):
    d = samples(delay)
    out = signal + [0.0] * (d * repeats)
    for r in range(1, repeats + 1):
        g = mix_level * feedback ** (r - 1)
        for i, s in enumerate(signal):
            out[i + d * r] += s * g
    return out


def loop(signal, crossfade=0.05):
    """Make a sound loop seamlessly: fold its tail over its head with a crossfade."""
    c = samples(crossfade)
    body = signal[:-c] if c else signal[:]
    for i in range(c):
        t = i / c
        body[i] = body[i] * t + signal[len(signal) - c + i] * (1 - t)
    return body


def mix(*parts):
    n = max(len(p) for p in parts)
    out = [0.0] * n
    for p in parts:
        for i, s in enumerate(p):
            out[i] += s
    return out


def gba(signal, gain=1.0, looping=False):
    """The GBA output stage: hold samples at the Direct Sound rate and
    quantize to 8 bits, normalized so the loudest point sits near full."""
    # The GBA's low mixing rate and simple output filter keep it warm: low-pass
    # first, so the hold and the 8-bit steps don't alias into hiss
    signal = svf(signal, 7000, 0.0, "low")
    peak = max(1e-6, max(abs(s) for s in signal))
    scale = min(gain / peak, 1.6) * 0.9
    out, held, acc = [], 0.0, 1.0
    step = GBA_RATE / RATE
    for s in signal:
        acc += step
        if acc >= 1.0:
            acc -= 1.0
            held = round(max(-1, min(1, s * scale)) * 127) / 127
        out.append(held)
    # Fade the last few ms so nothing clicks (loops join themselves instead)
    if not looping:
        fade = min(len(out), samples(0.004))
        for i in range(fade):
            out[-1 - i] *= i / fade
    return out


# --- The sounds -------------------------------------------------------------

def sounds():
    C4, E4, G4 = note("C4"), note("E4"), note("G4")
    C5, D5, E5, G5, A5, B5 = (note(x) for x in ("C5", "D5", "E5", "G5", "A5", "B5"))
    C6, E6, G6 = note("C6"), note("E6"), note("G6")

    def chime(freqs, step, length=0.35, ratio=3.5, index=1.8, volume=0.4):
        return mix(*[place(fm(f, length, ratio=ratio, index=index, volume=volume), k * step) for k, f in enumerate(freqs)])

    return {
        # Menus and the cursor: soft wooden ticks and bell chimes
        # A soft woody tick: a damped low pluck with a tiny click on top
        "cursor": gba(mix(pluck(note("E5"), 0.045, volume=0.6, brightness=0.15),
                          svf(noise(0.008, 0.5), 3000, 0.3, "band")), gain=0.32),
        "menu_move": gba(mix(fm(note("A5"), 0.06, ratio=4, index=1.6, volume=0.5, curve=3),
                             pluck(note("A4"), 0.06, volume=0.3, brightness=0.2)), gain=0.45),
        "menu_confirm": gba(chime([E5, B5], 0.045, length=0.14, ratio=3.5, index=1.6), gain=0.62),
        "menu_back": gba(chime([B5 / 2 * 1.0, E5 / 2], 0.045, length=0.12, ratio=2, index=1.0), gain=0.55),
        "error": gba(mix(fm(110, 0.25, ratio=1.41, index=4, index_decay=0.3, volume=0.5, curve=1.2),
                         fm(116, 0.25, ratio=1.41, index=3, index_decay=0.3, volume=0.3, curve=1.2)), gain=0.6),
        # Units
        "select": gba(mix(pluck(C5, 0.25, volume=0.4, brightness=0.6), place(pluck(G5, 0.25, volume=0.4, brightness=0.6, seed=2), 0.04),
                          place(fm(C6, 0.2, ratio=3.5, index=1.5, volume=0.25), 0.07)), gain=0.6),
        # Movement loops play for as long as a unit travels
        "move_foot": gba(loop(mix([0.0] * samples(1.0), *[place(mix(svf(noise(0.05, 0.6, seed=k), 1500 + 300 * (k % 2), 0.1, "band"),
                                                                    kick(0.06, 100 + 15 * (k % 2), 55, 0.35)), k * 0.24 + 0.02) for k in range(5)]), 0.03),
                         gain=0.5, looping=True),
        "move_wheels": gba(loop(mix(svf(steady_noise(1.05, 0.7), [260 + 120 * math.sin(i * TAU * 3 / RATE) for i in range(samples(1.05))], 0.5, "low"),
                                    *[place(fm(200 + 25 * (k % 3), 0.05, ratio=2.7, index=3, volume=0.12), k * 0.13 + 0.02) for k in range(8)])),
                           gain=0.5, looping=True),
        "move_air": gba(loop(svf(steady_noise(1.05, 0.8), [800 + 700 * abs(math.sin(i * TAU * 4.5 / RATE)) for i in range(samples(1.05))], 0.4, "band")),
                        gain=0.45, looping=True),
        "move_boat": gba(loop(svf(steady_noise(1.25, 0.8), [700 + 500 * (0.5 - 0.5 * math.cos(i * TAU * 0.8 / RATE)) for i in range(samples(1.25))], 0.3, "band"), 0.15),
                         gain=0.45, looping=True),
        "join": gba(mix(pluck(E5, 0.25, 0.4), place(pluck(B5, 0.3, 0.4, seed=3), 0.07)), gain=0.55),
        "load": gba(mix(pluck(G5, 0.2, 0.4), place(pluck(C5, 0.25, 0.4, seed=4), 0.06), place(kick(0.08, 120, 70, 0.3), 0.1)), gain=0.55),
        "unload": gba(mix(pluck(C5, 0.2, 0.4), place(pluck(G5, 0.25, 0.4, seed=5), 0.06)), gain=0.55),
        # Combat
        "hit": gba(mix(kick(0.18, 160, 50, 0.9), svf(noise(0.12, 0.8), 3500, 0.2, "band"),
                       fm(380, 0.2, ratio=1.4, index=5, index_decay=0.5, volume=0.35)), gain=0.9),
        "shot": gba(mix(svf(noise(0.18, 0.5, curve=1.5), sweep(5000, 1200, 0.18), 0.3, "band"),
                        fm(240, 0.22, ratio=1.0, index=6, index_decay=3, volume=0.4, slide_to=180)), gain=0.75),
        "boom": gba(echo(mix(kick(0.6, 120, 32, 1.0, curve=1.8), svf(noise(0.7, 0.9, curve=1.6), sweep(2500, 200, 0.7), 0.2, "low")),
                         delay=0.11, feedback=0.3, mix_level=0.25), gain=1.0),
        "defeat": gba(mix(svf(noise(0.5, 0.8, attack=0.01, curve=1.5), sweep(3000, 300, 0.5), 0.3, "low"),
                          fm(400, 0.45, ratio=0.5, index=2, volume=0.3, slide_to=110)), gain=0.75),
        "heal": gba(echo(chime([C6, E6, G6, C6 * 2], 0.07, length=0.4, ratio=3.5, index=1.4, volume=0.35), delay=0.1, feedback=0.4), gain=0.65),
        # Properties and money
        "capture_tick": gba(chime([E5, G5], 0.05, length=0.18, ratio=3.5, index=1.6), gain=0.55),
        "capture_done": gba(echo(mix(*[place(brass([f, f * 1.5], 0.3, volume=0.45), k * 0.11) for k, f in enumerate([C4, E4, G4])],
                                     place(brass([C5, E5, G5], 0.6, volume=0.5, attack=0.02), 0.33),
                                     place(fm(C6, 0.5, ratio=3.5, index=1.5, volume=0.2), 0.33)), delay=0.12, feedback=0.3, mix_level=0.25), gain=0.85),
        "build": gba(mix(fm(523, 0.3, ratio=2.76, index=4, index_decay=0.6, volume=0.5), svf(noise(0.04, 0.6), 4000, 0.2, "band"),
                         place(mix(fm(523, 0.35, ratio=2.76, index=4, index_decay=0.6, volume=0.5), svf(noise(0.04, 0.6, seed=2), 4000, 0.2, "band")), 0.16)), gain=0.7),
        "funds": gba(mix(fm(B5, 0.12, ratio=3, index=1.5, volume=0.45), place(fm(E6, 0.4, ratio=3, index=1.5, volume=0.45), 0.08)), gain=0.6),
        # Commanders and turns
        "power": gba(echo(mix(brass([C4, G4, C5], 1.0, volume=0.5, attack=0.5, cutoff=(200, 4000)),
                              svf(noise(1.0, 0.5, attack=0.6, curve=0.8), 6000, 0.1, "high"),
                              place(kick(0.4, 100, 40, 0.8), 0.55),
                              place(chime([C6, E6, G6, C6 * 2], 0.05, length=0.5, volume=0.25), 0.55)),
                          delay=0.12, feedback=0.35, mix_level=0.3), gain=0.95),
        "day": gba(echo(mix(place(brass([G4], 0.15, volume=0.45, attack=0.01), 0.0),
                            place(brass([C5, E5, G5], 0.55, volume=0.5, attack=0.02), 0.13),
                            place(kick(0.25, 110, 50, 0.4), 0.13)), delay=0.1, feedback=0.3, mix_level=0.25), gain=0.8),
        "turn_end": gba(chime([G5, E5, C5], 0.06, length=0.22, ratio=2, index=1.2), gain=0.5),
    }


def write_wav(path, data):
    with wave.open(str(path), "wb") as w:
        w.setnchannels(1)
        w.setsampwidth(2)
        w.setframerate(RATE)
        w.writeframes(b"".join(struct.pack("<h", int(max(-1, min(1, s)) * 32767)) for s in data))


def main():
    args = [a for a in sys.argv[1:] if not a.startswith("--") and (sys.argv.index(a) == 0 or sys.argv[sys.argv.index(a) - 1] != "--play")]
    out = Path(args[0]) if args else REPO / "assets" / "audio" / "sfx"
    out.mkdir(parents=True, exist_ok=True)
    play = sys.argv[sys.argv.index("--play") + 1] if "--play" in sys.argv else None
    all_sounds = sounds()
    for name, data in all_sounds.items():
        write_wav(out / f"{name}.wav", data)
    print(f"Wrote {len(all_sounds)} sounds to {out}")
    if play:
        subprocess.run(["afplay", str(out / f"{play}.wav")])


if __name__ == "__main__":
    main()
