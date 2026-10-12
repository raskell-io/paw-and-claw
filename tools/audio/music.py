#!/usr/bin/env python3
"""Compose and render the game's music, Game Boy Advance style.

Like the Advance Wars soundtracks, every commander has a looping theme that
carries their personality, each army has a power theme for while a CO Power
runs, and a short fanfare opens every battle. Melodies are original.

How a theme is made:
- Each army has a *style*: scale, chord progressions, instrument kit, bass
  and drum grooves and a base tempo (Western: bright major brass with a lute;
  Northern: minor marches with horns; Southern: mixolydian marimba and toms;
  Eastern: pentatonic flute and koto over taiko; Nether: harmonic-minor low
  brass and organ, slower and ominous).
- Each commander has a *motif* in four 2-bar phrases (statement, answer,
  B phrase, cadence) written in scale degrees, plus a key, tempo and swing
  from their personality. The composer develops the motif into a 16-bar
  loop: statement + answer, statement + cadence, B + B one step higher,
  statement + cadence.
- Everything is rendered with the instrument kit from tools/audio/sfx.py
  through the GBA output stage, and folded into a seamless loop.

Usage:
  python3 tools/audio/music.py [NAME ...]   # default: everything

Writes assets/audio/music/<name>.ogg (mono Vorbis via ffmpeg). Standard
library + ffmpeg.
"""

import math
import random
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
from sfx import RATE, TAU, svf, noise, kick, gba, write_wav  # noqa: E402

REPO = Path(__file__).resolve().parents[2]
OUT = REPO / "assets" / "audio" / "music"


def midi_freq(m):
    return 440.0 * 2 ** ((m - 69) / 12)


def note_midi(name):
    semis = {"C": 0, "D": 2, "E": 4, "F": 5, "G": 7, "A": 9, "B": 11}[name[0]]
    rest = name[1:]
    if rest[0] == "#":
        semis, rest = semis + 1, rest[1:]
    elif rest[0] == "b":
        semis, rest = semis - 1, rest[1:]
    return 12 * (int(rest) + 1) + semis


def samples(seconds):
    return int(seconds * RATE)


# --- Instruments -----------------------------------------------------------------

def sustain_env(i, n, attack, decay, sustain, release):
    a, d, r = max(1, int(attack * RATE)), max(1, int(decay * RATE)), max(1, int(release * RATE))
    e = i / a if i < a else (1 - (1 - sustain) * (i - a) / d if i < a + d else sustain)
    if i > n - r:
        e *= max(0.0, (n - i) / r)
    return e


def fm_voice(freq, length, volume, ratio, index, attack=0.012, decay=0.12, sustain=0.72, release=0.05,
             vibrato=0.0, index_hold=0.6):
    """FM tone with a sustain envelope and vibrato that creeps in on long notes."""
    n = samples(length)
    out, pc, pm = [], 0.0, 0.0
    for i in range(n):
        t = i / RATE
        vib = 1 + vibrato * min(1.0, max(0.0, t - 0.15) * 4) * math.sin(TAU * 5.5 * t)
        e = sustain_env(i, n, attack, decay, sustain, release)
        pm += TAU * freq * vib * ratio / RATE
        pc += TAU * freq * vib / RATE
        out.append(math.sin(pc + index * ((1 - index_hold) + index_hold * e) * math.sin(pm)) * e * volume)
    return out


def brass(freq, length, volume):
    return fm_voice(freq, length, volume, 1.0, 1.6, vibrato=0.006)


def horn(freq, length, volume):
    return fm_voice(freq, length, volume, 1.0, 1.0, attack=0.03, decay=0.2, sustain=0.8, vibrato=0.004)


def low_brass(freq, length, volume):
    return fm_voice(freq, length, volume, 1.0, 2.4, attack=0.02, decay=0.25, sustain=0.75, vibrato=0.003, index_hold=0.8)


def marimba(freq, length, volume):
    n = samples(min(length, 0.6))
    out, pc, pm = [], 0.0, 0.0
    for i in range(n):
        e = math.exp(-i / (0.16 * RATE))
        idx = 3.0 * math.exp(-i / (0.012 * RATE))
        pm += TAU * freq * 4 / RATE
        pc += TAU * freq / RATE
        out.append(math.sin(pc + idx * math.sin(pm)) * e * volume)
    return out


def flute(freq, length, volume, seed=0):
    rng = random.Random(seed)
    n = samples(length)
    out, p = [], 0.0
    breath = svf([rng.uniform(-1, 1) for _ in range(n)], freq * 2, 0.6, "band")
    for i in range(n):
        t = i / RATE
        vib = 1 + 0.008 * min(1.0, max(0.0, t - 0.2) * 3) * math.sin(TAU * 5.2 * t)
        p += TAU * freq * vib / RATE
        e = sustain_env(i, n, 0.04, 0.1, 0.85, 0.06)
        out.append((math.sin(p) + 0.18 * math.sin(2 * p) + 0.08 * breath[i] * (1.5 - e)) * e * volume)
    return out


def pluck(freq, length, volume, brightness=0.5, seed=1):
    """Karplus-Strong string: lute when soft, koto when bright."""
    rng = random.Random(seed)
    period = max(2, int(RATE / freq))
    buf = [rng.uniform(-1, 1) for _ in range(period)]
    n = samples(length)
    damp = 0.5 + 0.45 * brightness
    out = []
    for i in range(n):
        j = i % period
        s = buf[j]
        buf[j] = (damp * s + (1 - damp) * buf[(j + 1) % period]) * 0.997
        out.append(s * volume * max(0.0, 1 - i / n) ** 0.5)
    return out


def slap_bass(freq, length, volume):
    n = samples(length)
    out, pc, pm = [], 0.0, 0.0
    for i in range(n):
        e = sustain_env(i, n, 0.003, 0.09, 0.55, 0.03)
        idx = 0.8 + 3.2 * math.exp(-i / (0.03 * RATE))
        pm += TAU * freq / RATE
        pc += TAU * freq / RATE
        out.append(math.sin(pc + idx * math.sin(pm)) * e * volume)
    return out


def strings(freqs, length, volume):
    n = samples(length)
    phases = [[random.random(), random.random()] for _ in freqs]
    raw = []
    for i in range(n):
        s = 0.0
        for k, f in enumerate(freqs):
            for d, det in enumerate((1.004, 0.996)):
                phases[k][d] = (phases[k][d] + f * det / RATE) % 1.0
                s += 2 * phases[k][d] - 1
        raw.append(s / (2 * len(freqs)) * sustain_env(i, n, 0.18, 0.2, 0.85, 0.25))
    return [s * volume for s in svf(raw, 1800, 0.1, "low")]


def organ(freqs, length, volume):
    """Additive drawbar-ish organ: fundamental plus a few harmonics, slow swell."""
    n = samples(length)
    out = []
    for i in range(n):
        t = i / RATE
        s = 0.0
        for f in freqs:
            s += math.sin(TAU * f * t) + 0.5 * math.sin(TAU * 2 * f * t) + 0.25 * math.sin(TAU * 3 * f * t)
        out.append(s / (1.75 * len(freqs)) * sustain_env(i, n, 0.12, 0.2, 0.9, 0.2) * volume)
    return out


def snare(volume):
    body = [math.sin(TAU * 190 * i / RATE) * math.exp(-i / (0.03 * RATE)) * 0.6 for i in range(samples(0.16))]
    rattle = svf(noise(0.16, 1.0, curve=2.5, seed=11), 2500, 0.1, "high")
    return [(b + r * 0.7) * volume for b, r in zip(body, rattle)]


def hat(volume, length=0.05, seed=3):
    return [s * volume for s in svf(noise(length, 1.0, curve=3, seed=seed), 7000, 0.0, "high")]


def tom(pitch, volume):
    return kick(0.22, pitch * 1.6, pitch, volume, curve=2.2)


def woodblock(volume):
    return fm_voice(1400, 0.06, volume, 2.7, 2.0, attack=0.001, decay=0.05, sustain=0.0, release=0.01)


def crash(volume, length=1.2):
    return [s * volume for s in svf(noise(length, 1.0, curve=1.6, seed=21), 5000, 0.0, "high")]


def sax(freq, length, volume, seed=0):
    """Breathy FM reed: a growly attack, a round held tone and a little air."""
    tone = fm_voice(freq, length, volume, 1.0, 3.0, attack=0.025, decay=0.2, sustain=0.75, vibrato=0.01, index_hold=0.8)
    rng = random.Random(seed)
    air = svf([rng.uniform(-1, 1) for _ in range(len(tone))], freq * 3, 0.5, "band")
    n = len(tone)
    return [t + a * 0.05 * volume * sustain_env(i, n, 0.02, 0.1, 0.6, 0.05) for i, (t, a) in enumerate(zip(tone, air))]


def harmonica(freq, length, volume, seed=0):
    """Reedy harmonica: a hollow FM reed that bends up into the note, with a
    wide, wavering vibrato."""
    n = samples(length)
    out, pc, pm = [], 0.0, 0.0
    for i in range(n):
        t = i / RATE
        bend = 1 - 0.04 * math.exp(-t / 0.05)
        vib = 1 + 0.012 * min(1.0, max(0.0, t - 0.1) * 3) * math.sin(TAU * 6.2 * t)
        f = freq * bend * vib
        e = sustain_env(i, n, 0.03, 0.15, 0.8, 0.05)
        pm += TAU * f * 2 / RATE
        pc += TAU * f / RATE
        out.append(math.sin(pc + 1.3 * (0.5 + 0.5 * e) * math.sin(pm)) * e * volume)
    return out


def strum(freqs, length, volume, brightness=0.7, seed=0, spread=0.012):
    """A guitar strum: the chord's strings plucked one after another."""
    out = [0.0] * (samples(length) + samples(spread * len(freqs)))
    for k, f in enumerate(freqs):
        s = pluck(f, length, volume, brightness=brightness, seed=seed * 7 + k)
        o = samples(spread * k)
        for i, x in enumerate(s):
            out[o + i] += x
    return out


def bright_brass(freq, length, volume):
    """Brighter, punchier synth-brass lead for upbeat themes."""
    return fm_voice(freq, length, volume, 1.0, 2.2, attack=0.006, decay=0.08, sustain=0.7, vibrato=0.005)


def horn_brass(freq, length, volume):
    """Pompous horn doubled by brass an octave up: a whole section."""
    a = horn(freq, length, volume * 0.8)
    b = brass(freq * 2, length, volume * 0.35)
    return [x + y for x, y in zip(a, b)]


LEADS = {"harmonica": harmonica, "sax": sax, "bright_brass": bright_brass, "horn_brass": horn_brass, "brass": brass, "horn": horn, "low_brass": low_brass, "marimba": marimba, "flute": flute}


# --- Scales, chords and arrangement ----------------------------------------------

SCALES = {
    "major": [0, 2, 4, 5, 7, 9, 11],
    "minor": [0, 2, 3, 5, 7, 8, 10],
    "dorian": [0, 2, 3, 5, 7, 9, 10],
    "mixolydian": [0, 2, 4, 5, 7, 9, 10],
    "harmonic_minor": [0, 2, 3, 5, 7, 8, 11],
    "pentatonic": [0, 2, 4, 7, 9],
    "minor_pentatonic": [0, 3, 5, 7, 10],
}


def degree_semis(scale, degree):
    steps = SCALES[scale]
    octave, idx = divmod(degree, len(steps))
    return 12 * octave + steps[idx]


def chord(harmony, degree, major_five=False, sevenths=False):
    """Triad (or seventh chord) stacked in thirds on a scale degree, as
    semitones from the key."""
    tones = [degree_semis(harmony, degree + k) for k in ((0, 2, 4, 6) if sevenths else (0, 2, 4))]
    if major_five and degree % 7 == 4:
        tones[1] = tones[0] + 4
    return tones


STYLES = {
    "western": dict(scale="major", harmony="major", bpm=138, lead="brass", lead_octave=0,
                    prog_a=[0, 4, 5, 3, 0, 4, 3, 4], prog_b=[5, 3, 0, 4, 5, 3, 1, 4],
                    pad="strings", counter="lute", bass=[0, 12, 7, 12, 0, 12, 7, 12], drums="march"),
    "northern": dict(scale="minor", harmony="minor", bpm=116, lead="horn", lead_octave=0, major_five=True,
                     prog_a=[0, 5, 2, 6, 0, 5, 3, 4], prog_b=[3, 0, 5, 2, 3, 0, 1, 4],
                     pad="strings", counter="bells", bass=[0, None, 7, None, 0, None, 7, 7], drums="military"),
    "southern": dict(scale="mixolydian", harmony="mixolydian", bpm=126, lead="marimba", lead_octave=0,
                     prog_a=[0, 6, 3, 0, 0, 6, 3, 4], prog_b=[3, 0, 6, 3, 3, 0, 1, 4],
                     pad="strings", counter="marimba", bass=[0, None, 0, 7, None, 7, 10, 12], drums="toms"),
    "eastern": dict(scale="pentatonic", harmony="major", bpm=120, lead="flute", lead_octave=0,
                    prog_a=[0, 5, 3, 4, 0, 5, 1, 4], prog_b=[3, 4, 2, 5, 3, 4, 1, 4],
                    pad="strings", counter="koto", bass=[0, 7, 12, 7, 0, 7, 12, 7], drums="taiko"),
    "nether": dict(scale="harmonic_minor", harmony="harmonic_minor", bpm=108, lead="low_brass", lead_octave=0,
                   major_five=True, prog_a=[0, 5, 3, 4, 0, 5, 1, 4], prog_b=[5, 3, 0, 4, 5, 1, 3, 4],
                   pad="organ", counter="none", bass=[0, 0, 12, 0, 0, 1, 0, 6], drums="heavy"),
}

TEMPO = {"Methodical": 0, "Aggressive": 10, "Reckless": 18, "Cautious": -8}


def develop(motif):
    """Four 2-bar phrases -> 16 bars: A A' | A C | B B+1 | A C."""
    m, a, b, c = motif
    for name, phrase in zip("mabc", motif):
        assert abs(sum(l for _, l in phrase) - 8) < 1e-6, f"phrase {name} isn't 8 beats: {phrase}"
    up = [(None if d is None else d + 1, l) for d, l in b]
    return m + a + m + c + b + up + m + c


def render(spec):
    """Render a composed theme: spec has style, key (midi of degree 0 in the
    melody's octave), bpm, swing, melody timeline [(degree|None, beats)] and a
    16-chord progression (or bars for jingles)."""
    style = STYLES[spec["style"]]
    bpm = spec["bpm"]
    beat = 60.0 / bpm
    swing = spec.get("swing", 0.0)
    key = spec["key"]
    bars = len(spec["progression"])
    loop_len = samples(bars * 4 * beat)
    buf = [0.0] * loop_len

    def when(pos):
        # Swing pushes off-beat eighths late
        if swing and abs(pos % 1 - 0.5) < 1e-6:
            pos += swing * 0.5
        return samples(pos * beat)

    def add(sound, pos):
        start = when(pos)
        if start + len(sound) > len(buf):
            buf.extend([0.0] * (start + len(sound) - len(buf)))
        for i, s in enumerate(sound):
            buf[start + i] += s

    lead_fn = LEADS[spec.get("lead", style["lead"])]
    lead_volume = spec.get("lead_volume", 0.34)
    pos = 0.0
    lead_buf_start = len(buf)
    for k, (d, length) in enumerate(spec["melody"]):
        if d is not None:
            m = key + degree_semis(style["scale"], d) + 12 * style["lead_octave"]
            f = midi_freq(m)
            if lead_fn in (flute, sax, harmonica):
                add(lead_fn(f, length * beat * 0.95, lead_volume, seed=k), pos)
            else:
                add(lead_fn(f, length * beat * 0.95, lead_volume), pos)
                # The marimba gets a soft horn an octave down to carry long notes
                if lead_fn is marimba and length >= 1:
                    add(horn(f / 2, length * beat * 0.9, lead_volume * 0.35), pos)
        pos += length
    # A light echo on everything so far (the lead)
    d = samples(beat * 0.75)
    lead_part = buf[:]
    for i, s in enumerate(lead_part):
        if s and i + d < len(buf):
            buf[i + d] += s * 0.2
    del lead_buf_start

    key_root = key - 12  # chords and pads sit an octave below the melody
    for b, degree in enumerate(spec["progression"]):
        tones = chord(style["harmony"], degree, style.get("major_five", False), spec.get("sevenths", False))
        root = key_root + tones[0]
        pad_freqs = [midi_freq(key_root + t) for t in tones]
        next_tones = chord(style["harmony"], spec["progression"][(b + 1) % bars], style.get("major_five", False))
        if style["pad"] == "organ":
            add(organ(pad_freqs, 4 * beat, 0.13), b * 4)
        else:
            add(strings(pad_freqs, 4 * beat, 0.15), b * 4)
        bass = spec.get("bass", style["bass"])
        if bass == "walk":
            # Walking bass: root, third, fifth, then a half step into the next chord
            next_root = key_root + next_tones[0]
            for k, m in enumerate((root, key_root + tones[1], key_root + tones[2], next_root - 1)):
                add(slap_bass(midi_freq(m - 24), beat * 0.9, 0.4), b * 4 + k)
        else:
            for k, semis in enumerate(bass):
                if semis is not None:
                    add(slap_bass(midi_freq(root - 24 + semis), beat * 0.45, 0.42), b * 4 + k * 0.5)
        counter = spec.get("counter", style["counter"])
        up = [midi_freq(key_root + 12 + t) for t in tones]
        if counter == "lute":
            for k in range(4):
                add(pluck(up[k % 3], beat * 0.9, 0.12, brightness=0.4, seed=b * 4 + k), b * 4 + k + 0.5)
        elif counter == "koto":
            for k in range(8):
                add(pluck(up[(0, 1, 2, 1)[k % 4]] * (2 if k % 4 == 2 else 1), beat * 0.6, 0.1, brightness=0.9, seed=b * 8 + k), b * 4 + k * 0.5)
        elif counter == "marimba":
            for k in range(8):
                if k % 3 != 2:
                    add(marimba(up[(k * 2) % 3], beat * 0.5, 0.1), b * 4 + k * 0.5)
        elif counter == "arp":
            # Bright synth-pluck arpeggio on sixteenths, up and down the chord
            for k in range(16):
                f = up[(0, 1, 2, 1)[k % 4]] * (2 if k % 8 >= 4 else 1)
                add(pluck(f, beat * 0.3, 0.07, brightness=0.8, seed=b * 16 + k), b * 4 + k * 0.25)
        elif counter == "strum":
            # Boom-chick: the bass takes the beats, the guitar strums the off-beats
            for k in range(4):
                add(strum(up, beat * 0.4, 0.06, seed=b * 4 + k), b * 4 + k + 0.5)
        elif counter == "rasgueado":
            # Flamenco strums, accented on the 3-3-2 of the bar
            for k, v in ((0, 0.09), (1.5, 0.07), (3, 0.08), (3.5, 0.05)):
                add(strum(up, beat * 0.6, v, brightness=0.85, seed=b * 4 + int(k * 2), spread=0.008), b * 4 + k)
        elif counter == "bells":
            add(fm_voice(up[2] * 2, beat * 1.5, 0.06, 3.5, 1.4, attack=0.002, decay=0.4, sustain=0.0, release=0.05), b * 4 + 2)
        drum_bar(add, spec.get("drums", style["drums"]), b, bars)

    mixed = buf
    looped = mixed[:loop_len]
    if spec.get("loop", True):
        for i, s in enumerate(mixed[loop_len:]):
            looped[i % loop_len] += s
    else:
        looped = mixed
    return gba(looped, gain=0.9, looping=spec.get("loop", True))


def drum_bar(add, kind, b, bars):
    phrase_end = b % 8 == 7
    o = b * 4
    if kind in ("march", "military", "drive"):
        add(kick(0.2, 130, 50, 0.6, curve=3), o)
        add(kick(0.2, 130, 50, 0.5, curve=3), o + 2)
        add(snare(0.42), o + 1)
        if phrase_end:
            for k in range(6):
                add(snare(0.24 + 0.04 * k), o + 2.5 + k * 0.25)
        else:
            add(snare(0.42), o + 3)
        if kind == "military":
            # Snare ruffs leading into beats 2 and 4
            add(snare(0.16), o + 0.75)
            add(snare(0.16), o + 2.75)
        step = 0.25 if kind == "drive" else 0.5
        for k in range(int(4 / step)):
            add(hat(0.14 if (k * step) % 1 else 0.09, seed=b * 16 + k), o + k * step)
        if kind == "drive" and b % 4 == 0:
            add(crash(0.12), o)
    elif kind == "rock":
        # Kick on 1, the and of 2, and 3; snare on 2 and 4; open hat on the and of 4
        for k, v in ((0, 0.65), (1.5, 0.45), (2, 0.6)):
            add(kick(0.18, 140, 50, v, curve=3), o + k)
        add(snare(0.45), o + 1)
        add(snare(0.45), o + 3)
        for k in range(8):
            add(hat(0.13 if k % 2 else 0.08, seed=b * 8 + k), o + k * 0.5)
        add(hat(0.12, length=0.18, seed=b + 99), o + 3.5)
        if b % 4 == 0:
            add(crash(0.1, 1.0), o)
        if phrase_end:
            for k in range(4):
                add(tom(160 - 20 * k, 0.32), o + 3 + k * 0.25)
    elif kind == "swing":
        # Ride on 1, 2, the and of 2, 3, 4, the and of 4; soft kick; brushed backbeat
        for k in (0, 1, 1.5, 2, 3, 3.5):
            add(hat(0.1 if k % 1 else 0.13, length=0.14, seed=b * 8 + int(k * 2)), o + k)
        for k in (0, 2):
            add(kick(0.18, 100, 50, 0.35, curve=3), o + k)
        for k in (1, 3):
            add([x * 0.7 for x in svf(noise(0.18, 0.35, attack=0.01, curve=1.5, seed=b + k), 3000, 0.1, "band")], o + k)
        if phrase_end:
            add(snare(0.3), o + 3.5)
    elif kind == "train":
        # Country train beat: brushed snare on every eighth, accents on 2 and 4
        add(kick(0.18, 100, 50, 0.45, curve=3), o)
        add(kick(0.18, 100, 50, 0.4, curve=3), o + 2)
        for k in range(8):
            accent = k in (2, 6)
            brush = svf(noise(0.1, 0.5 if accent else 0.22, attack=0.005, curve=2, seed=b * 8 + k), 3500, 0.1, "band")
            add(brush, o + k * 0.5)
        if phrase_end:
            add(snare(0.3), o + 3.5)
    elif kind == "flamenco":
        # Castanet clicks and hand claps over a light kick
        add(kick(0.2, 110, 50, 0.45, curve=3), o)
        add(kick(0.2, 110, 50, 0.35, curve=3), o + 2)
        for k in (0, 0.25, 1.5, 2, 2.25, 3.5):
            add(woodblock(0.1), o + k)
        for k in (1, 3):
            for r in range(3):
                add(svf(noise(0.05, 0.35, curve=2, seed=b * 9 + k * 3 + r), 1500, 0.2, "band"), o + k + r * 0.03)
        if phrase_end:
            for k in range(8):
                add(woodblock(0.07 + 0.01 * k), o + 2 + k * 0.25)
    elif kind == "toms":
        add(kick(0.2, 120, 50, 0.6, curve=3), o)
        add(kick(0.2, 120, 50, 0.45, curve=3), o + 1.5)
        add(kick(0.2, 120, 50, 0.5, curve=3), o + 2.5)
        add(snare(0.3), o + 1)
        add(snare(0.3), o + 3)
        for k, p in ((0.5, 220), (2, 180), (3.5, 150)):
            add(tom(p, 0.25), o + k)
        for k in range(16):
            add(hat(0.06 + 0.04 * (k % 2), length=0.03, seed=b * 16 + k), o + k * 0.25)
    elif kind == "taiko":
        add(tom(70, 0.7), o)
        add(tom(70, 0.45), o + 2.5)
        add(tom(110, 0.35), o + 3)
        for k in (1, 3):
            add(woodblock(0.12), o + k)
        for k in range(8):
            add(hat(0.07, seed=b * 8 + k), o + k * 0.5 + 0.25)
        if phrase_end:
            for k in range(4):
                add(tom(90 + 15 * k, 0.3), o + 2 + k * 0.5)
    elif kind == "heavy":
        add(kick(0.3, 110, 40, 0.75, curve=2.5), o)
        add(kick(0.3, 110, 40, 0.55, curve=2.5), o + 2.5)
        add(snare(0.45), o + 2)
        for k in range(4):
            add(hat(0.08, seed=b * 4 + k), o + k + 0.5)
        if phrase_end:
            for k in range(4):
                add(tom(120 - 15 * k, 0.4), o + 3 + k * 0.25)


# --- Commanders ------------------------------------------------------------------
# key: the motif's tonic (degree 0), around the middle of the lead's range.
# motif: (statement, answer, B phrase, cadence), each 8 beats of (degree, beats).
# Degrees count steps of the army's scale from the tonic (negative = below).

COMMANDERS = {
    # Dusty plays like Andy: cheerful, fast pop-rock with a bright lead, hooks
    # that land off the beat, a pumping bass and a rock beat
    "dusty": dict(army="western", personality="Methodical", key="C5", bpm=152, lead="bright_brass",
                  drums="rock", bass=[0, 0, 12, 0, 0, 12, 7, 0], counter="arp", motif=(
        [(0, .5), (2, .5), (4, .5), (5, 1), (4, .5), (2, .5), (4, 1.5), (7, .5), (None, .5), (7, .5), (8, .5), (9, .5), (7, .5)],
        [(5, .5), (4, .5), (2, .5), (4, 1), (2, .5), (1, .5), (2, 1.5), (0, .5), (1, 1), (None, 1.5)],
        [(3, .5), (3, .5), (4, .5), (5, 1), (4, .5), (3, .5), (2, 1.5), (5, .5), (4, .5), (2, .5), (4, .5), (5, .5), (7, .5)],
        [(4, .5), (5, .5), (7, 1), (5, .5), (4, .5), (2, 1), (1, .5), (2, .5), (1, 1), (None, 2)])),
    # Frost plays like Olaf: a pompous, blustering minor-key march with dotted
    # brass rhythms, oom-pah bass, snare ruffs and grand strings
    "frost": dict(army="northern", personality="Methodical", key="D5", bpm=124, lead="horn_brass",
                  bass=[0, None, 7, None, 0, None, 7, None], motif=(
        [(0, .75), (0, .25), (4, 1), (4, .75), (3, .25), (2, .75), (1, .25), (2, 1), (0, 1), (-3, 2)],
        [(2, .75), (2, .25), (5, 1), (5, .75), (4, .25), (3, .75), (2, .25), (3, 1), (1, 1), (4, 2)],
        [(7, 1.5), (6, .5), (5, .75), (4, .25), (5, 1), (3, 1.5), (2, .5), (3, .75), (4, .25), (2, 1)],
        [(4, .75), (4, .25), (3, .75), (2, .25), (1, 1), (2, .75), (1, .25), (0, 1), (-3, 2), (None, 1)])),
    # Talon plays like Max: brash, swaggering bluesy rock with a heavy bounce
    "talon_co": dict(army="western", personality="Aggressive", key="G4", bpm=142, swing=0.1, scale="mixolydian",
                     lead="bright_brass", drums="rock", bass=[0, 12, 10, 12, 0, 12, 7, 10],
                     prog=[0, 6, 3, 0, 0, 6, 3, 4, 3, 3, 0, 0, 6, 6, 4, 4], motif=(
        [(0, .5), (2, .5), (4, 1), (6, .5), (4, 1), (2, .5), (4, 1), (0, 1), (None, 2)],
        [(4, .5), (4, .5), (6, 1), (7, .5), (6, 1), (4, .5), (3, 1), (2, 1), (None, 2)],
        [(7, 1), (7, .5), (6, .5), (4, 1), (6, 1.5), (7, .5), (9, 1), (7, 2)],
        [(6, .5), (4, .5), (3, .5), (2, .5), (0, 1), (1, 1), (2, 1), (4, 2), (None, 1)])),
    # Bandit plays like Sami: cool, swinging jazz with a sax lead, walking
    # bass, seventh chords and brushes
    "bandit": dict(army="western", personality="Reckless", key="F4", bpm=150, swing=0.22, lead="sax",
                   drums="swing", bass="walk", sevenths=True, counter="none",
                   prog=[1, 4, 0, 5, 1, 4, 0, 0, 3, 3, 2, 5, 1, 4, 0, 4], motif=(
        [(4, .5), (None, .5), (4, .5), (3, .5), (2, 1), (0, .5), (2, 1.5), (None, .5), (4, .5), (6, 1), (5, 1)],
        [(4, .5), (None, .5), (4, .5), (5, .5), (7, 1), (5, .5), (4, 1.5), (3, 1), (2, 2)],
        [(9, 1), (7, .5), (6, .5), (5, 1), (4, .5), (2, 1.5), (3, 1), (1, 2)],
        [(2, .5), (4, .5), (3, .5), (2, .5), (0, 1), (1, 1.5), (None, 3.5)])),
    # Prowler plays like Eagle: heroic, soaring and fast, in a minor key over
    # driving drums
    "prowler": dict(army="southern", personality="Aggressive", key="E5", bpm=154, scale="minor",
                    lead="bright_brass", drums="drive", counter="arp", bass=[0, 0, 12, 0, 0, 12, 0, 7],
                    prog=[0, 5, 6, 0, 0, 5, 6, 4, 5, 6, 0, 0, 3, 4, 5, 4], motif=(
        [(0, .5), (4, .5), (7, 1.5), (6, .5), (7, .5), (9, .5), (7, 1), (4, 1), (5, 1), (4, 1)],
        [(5, .5), (6, .5), (7, 1.5), (5, .5), (4, 1), (3, .5), (2, .5), (3, 1), (4, 2)],
        [(9, 1.5), (8, .5), (7, 1), (6, 1), (7, 1.5), (5, .5), (4, 2)],
        [(6, 1), (7, .5), (6, .5), (4, 1), (3, 1), (1, 1), (4, 3)])),
    # Hivemind plays like Sturm: menacing and slow, low brass and organ,
    # creeping half steps, heavy drums
    "hivemind": dict(army="nether", personality="Methodical", key="D5", bpm=112, motif=(
        [(0, 1.5), (0, .5), (1, 1), (0, 1), (-1, 1.5), (0, .5), (4, 2)],
        [(5, 1.5), (4, .5), (3, 1), (2, 1), (1, 1.5), (2, .5), (-1, 2)],
        [(7, 1.5), (6, .5), (5, 1), (4, 1), (5, 1.5), (3, .5), (4, 2)],
        [(3, 1), (2, 1), (1, 1), (0, 1), (-1, 3), (None, 1)])),
    # Kira plays like Kanbei: a grand imperial march, pentatonic melody on a
    # horn section over koto and taiko
    "kira": dict(army="eastern", personality="Aggressive", key="D5", bpm=138, lead="horn_brass", motif=(
        [(0, .5), (0, .5), (2, 1), (1, .5), (2, .5), (4, 1), (5, 1.5), (4, .5), (2, 2)],
        [(3, .5), (3, .5), (4, 1), (3, .5), (2, .5), (1, 1), (2, 1.5), (1, .5), (0, 2)],
        [(5, 1), (6, 1), (7, 1.5), (6, .5), (5, 1), (4, 1), (5, 2)],
        [(4, 1), (3, .5), (2, .5), (1, 1), (2, 1), (1, 3), (None, 1)])),
    # Tanuki plays like Sonja: mysterious and clever, staccato minor-pentatonic
    # flute, light and sly
    "tanuki": dict(army="eastern", personality="Methodical", key="A4", bpm=126, scale="minor_pentatonic", motif=(
        [(0, .5), (None, .5), (2, .5), (None, .5), (3, .5), (2, .5), (0, .5), (None, .5), (4, 1), (3, .5), (2, .5), (0, 2)],
        [(1, .5), (None, .5), (3, .5), (None, .5), (4, .5), (3, .5), (1, .5), (None, .5), (2, 1), (1, .5), (0, .5), (-1, 2)],
        [(5, 1), (4, .5), (3, .5), (4, 1), (2, 1), (3, .5), (4, .5), (5, 1), (3, 2)],
        [(2, .5), (3, .5), (2, .5), (1, .5), (0, 1), (-1, 1), (0, 2), (None, 2)])),
    # Lionheart plays like Javier: Spanish and chivalric, a trumpet over
    # flamenco strums, castanets and the Andalusian descent (i VII VI V)
    "lionheart": dict(army="southern", personality="Cautious", key="A4", bpm=136, scale="harmonic_minor", harmony="minor",
                      lead="bright_brass", counter="rasgueado", drums="flamenco", bass=[0, None, 0, 7, None, 0, 7, None],
                      prog=[0, 6, 5, 4, 0, 6, 5, 4, 3, 0, 4, 0, 3, 0, 5, 4], motif=(
        [(4, .5), (5, .5), (4, .5), (3, .5), (2, 1), (1, .5), (2, .5), (3, 1), (2, 1), (1, 2)],
        [(1, .5), (2, .5), (1, .5), (0, .5), (-1, 1), (0, .5), (1, .5), (2, 1), (1, 1), (-1, 2)],
        [(7, 1.5), (6, .5), (5, 1), (4, 1), (5, .5), (4, .5), (3, .5), (2, .5), (1, 2)],
        [(3, .5), (2, .5), (1, 1), (0, 1), (1, .5), (0, .5), (-1, 1), (-3, 2), (None, 1)])),
    # Grimjaw plays like Grit: laid-back country western, a harmonica drawl
    # over a swung train beat, boom-chick bass and guitar
    "grimjaw": dict(army="northern", personality="Cautious", key="E5", bpm=116, swing=0.22, scale="major",
                    lead="harmonica", counter="strum", drums="train", bass=[0, None, 7, None, 0, None, 7, None],
                    prog=[0, 0, 3, 0, 4, 4, 0, 4, 3, 3, 0, 0, 1, 4, 0, 4], motif=(
        [(4, 1), (2, .5), (4, .5), (5, 1), (4, .5), (2, .5), (0, 2), (None, 2)],
        [(4, 1), (2, .5), (4, .5), (5, 1), (7, .5), (5, .5), (4, 2), (None, 2)],
        [(7, 1), (8, .5), (7, .5), (5, 1), (4, 1), (5, .5), (4, .5), (2, 1), (1, 2)],
        [(2, .5), (4, .5), (2, .5), (0, .5), (-1, 1), (0, 1), (1, 2), (None, 2)])),
}

# Each army's power theme: its first commander's motif, faster, a step
# higher and over driving drums (AW plays a power theme while a CO Power runs)
# CO Power themes: each army's power music is a faster, higher, harder-driving
# arrangement of one of its commanders' themes (AW plays power music as the
# battle's climax)
POWER = {"western": "dusty", "northern": "grimjaw", "southern": "lionheart", "eastern": "kira", "nether": "hivemind"}


def commander_spec(name, co, power=False):
    style = STYLES[co["army"]]
    spec = dict(style=co["army"], key=note_midi(co["key"]), swing=co.get("swing", 0.0),
                bpm=style["bpm"] + TEMPO[co["personality"]],
                melody=develop(co["motif"]), progression=style["prog_a"] + style["prog_b"])
    if "scale" in co:
        spec["style"] = co["army"] + "_" + co["scale"]
        harmony = co.get("harmony", {"minor_pentatonic": "minor", "pentatonic": "major"}.get(co["scale"], co["scale"]))
        STYLES.setdefault(spec["style"], dict(style, scale=co["scale"], harmony=harmony,
                                              major_five=harmony in ("minor", "harmonic_minor")))
    if "prog" in co:
        spec["progression"] = co["prog"]
    for field in ("drums", "lead", "bpm", "bass", "counter", "sevenths"):
        if field in co:
            spec[field] = co[field]
    if power:
        spec["bpm"] += 26
        spec["key"] += 2
        spec["drums"] = "drive" if co["army"] in ("western", "northern", "nether") else spec.get("drums", style["drums"])
        spec["lead_volume"] = 0.36
    return spec


def battle_start_spec():
    """A short brass fanfare for the match intro (about 2.6 s, not looped)."""
    return dict(style="western", key=note_midi("C5"), bpm=140, loop=False, drums="none",
                melody=[(-3, .5), (-3, .25), (-3, .25), (0, 1), (2, .5), (1, .5), (0, .5), (4, 2.5)],
                progression=[0, 4])


def victory_spec():
    """Victory: a bright rising fanfare that lands on a long high tonic (about 4.5 s)."""
    return dict(key=note_midi("C5"), bpm=126, scale="major",
                melody=[(0, .5), (2, .5), (4, .5), (7, 1), (4, .5), (7, .5), (9, .5), (11, .5), (14, 3)])


def defeat_spec():
    """Defeat: a slow minor lament falling to a low, soft close (about 6 s)."""
    return dict(key=note_midi("A4"), bpm=72, scale="minor", soft=True,
                melody=[(4, 1), (3, .5), (2, .5), (1, 1), (0, .5), (-1, .5), (0, 3)])


def jingle(spec):
    """Render a fanfare: melody over brass chord stabs, a timpani roll and a crash."""
    beat = 60.0 / spec["bpm"]
    key = spec["key"]
    scale = spec.get("scale", "major")
    if spec.get("soft"):
        return lament(spec)
    buf = [0.0] * samples(8 * beat)

    def add(sound, pos):
        start = samples(pos * beat)
        if start + len(sound) > len(buf):
            buf.extend([0.0] * (start + len(sound) - len(buf)))
        for i, s in enumerate(sound):
            buf[start + i] += s

    pos = 0.0
    for d, length in spec["melody"]:
        add(brass(midi_freq(key + degree_semis(scale, d)), length * beat * 0.95, 0.4), pos)
        pos += length
    end = sum(length for _, length in spec["melody"])
    stabs = ((0, 0, 0.4), (0.5, 0, 0.2), (0.75, 0, 0.2), (1, 0, 0.9), (2, 4, 0.9), (3, 0, 3))
    if end > 6:
        # Longer fanfares: hold the last chord under the final note
        stabs = stabs[:-1] + ((3, 4, 0.9), (4, 0, end - 4 + 1))
    for at, degree, length in stabs:
        for t in chord(scale, degree):
            add(brass(midi_freq(key - 12 + t), length * beat, 0.16), at)
        add(slap_bass(midi_freq(key - 24 + chord(scale, degree)[0]), min(length, 1) * beat, 0.45), at)
    for k in range(12):
        add(kick(0.15, 110, 70, 0.25 + 0.03 * k, curve=2), 1.5 + k * 0.125)
    add(kick(0.5, 120, 40, 0.8), 3)
    add(crash(0.25, 2.5), 3)
    return gba(buf, gain=0.9)


def lament(spec):
    """A slow, soft minor phrase: horn melody over held low strings, no drums."""
    beat = 60.0 / spec["bpm"]
    key = spec["key"]
    scale = spec.get("scale", "minor")
    buf = []

    def add(sound, pos):
        start = samples(pos * beat)
        if start + len(sound) > len(buf):
            buf.extend([0.0] * (start + len(sound) - len(buf)))
        for i, x in enumerate(sound):
            buf[start + i] += x

    pos = 0.0
    for d, length in spec["melody"]:
        add(horn(midi_freq(key + degree_semis(scale, d)), length * beat * 0.98, 0.34), pos)
        pos += length
    # Held chords: i, then iv, then i, the bass a slow fall
    for at, degree, length in ((0, 0, 2), (2, 3, 2), (4, 0, pos - 4 + 1.5)):
        freqs = [midi_freq(key - 12 + t) for t in chord("minor", degree)]
        add(strings(freqs, length * beat, 0.14), at)
        add(low_brass(midi_freq(key - 24 + chord("minor", degree)[0]), length * beat, 0.2), at)
    add(tom(70, 0.35), 0)
    add(tom(60, 0.3), pos - 3)
    return gba(buf, gain=0.85)


def all_pieces():
    pieces = {}
    for name, co in COMMANDERS.items():
        pieces[f"co_{name}"] = lambda co=co, name=name: render(commander_spec(name, co))
    for army, lead in POWER.items():
        pieces[f"power_{army}"] = lambda lead=lead: render(commander_spec(lead, COMMANDERS[lead], power=True))
    pieces["battle_start"] = lambda: jingle(battle_start_spec())
    pieces["victory"] = lambda: jingle(victory_spec())
    pieces["defeat"] = lambda: jingle(defeat_spec())
    pieces["title"] = lambda: render(title_spec())
    pieces["intro"] = lambda: intro_song()
    return pieces


def intro_song():
    """The animated intro's song: an anime-opening style J-rock rush at 168
    BPM in E minor, not looped. A drum fill count-in, a driving verse over
    power-chord eighths, a soaring chorus a step up the scale, and a big final
    hit for the title (about 28 s)."""
    bpm = 168
    beat = 60.0 / bpm
    key = note_midi("E5")
    scale = "minor"
    buf = []

    def add(sound, pos):
        start = samples(pos * beat)
        if start + len(sound) > len(buf):
            buf.extend([0.0] * (start + len(sound) - len(buf)))
        for i, x in enumerate(sound):
            buf[start + i] += x

    def melody(phrase, at, fn=bright_brass, volume=0.34, octave=0):
        pos = at
        for d, length in phrase:
            if d is not None:
                add(fn(midi_freq(key + degree_semis(scale, d) + 12 * octave), length * beat * 0.92, volume), pos)
            pos += length
        return pos

    def power_chords(degrees, at, bars):
        # Distorted-ish guitar: detuned saws root + fifth, chugging eighths
        for b in range(bars):
            d = degrees[b % len(degrees)]
            root = key - 24 + degree_semis(scale, d)
            for k in range(8):
                v = 0.11 if k % 2 == 0 else 0.08
                add(brass_chord([midi_freq(root), midi_freq(root + 7), midi_freq(root + 12)], beat * 0.45, v), at + b * 4 + k * 0.5)
            for k in range(8):
                add(slap_bass(midi_freq(root - 12), beat * 0.45, 0.4), at + b * 4 + k * 0.5)

    def rock_beat(at, bars, fill_last=True):
        for b in range(bars):
            o = at + b * 4
            for k in (0, 1.5, 2, 2.5):
                add(kick(0.18, 150, 50, 0.6, curve=3), o + k)
            add(snare(0.45), o + 1)
            add(snare(0.45), o + 3)
            for k in range(8):
                add(hat(0.12 if k % 2 else 0.08, seed=b * 8 + k), o + k * 0.5)
            if b % 4 == 0:
                add(crash(0.12, 1.0), o)
            if fill_last and b == bars - 1:
                for k in range(8):
                    add(tom(200 - 15 * k, 0.32), o + 2 + k * 0.25)

    t = 0
    # Count-in: a snare roll building to a crash
    for k in range(16):
        add(snare(0.12 + 0.02 * k), t + k * 0.25)
    t += 4
    # Intro riff (4 bars): the hook on brass over power chords
    riff = [(0, .5), (2, .5), (4, .5), (7, 1), (6, .5), (4, .5), (2, .5)]
    power_chords([0, 0, 5, 6], t, 4)
    rock_beat(t, 4)
    for b in range(4):
        melody(riff if b % 2 == 0 else [(4, .5), (6, .5), (7, .5), (9, 1), (8, .5), (7, .5), (4, .5)], t + b * 4)
    t += 16
    # Verse (8 bars): a driving line, lower and tighter
    power_chords([0, 5, 3, 6, 0, 5, 3, 4], t, 8)
    rock_beat(t, 8)
    verse = [
        [(4, .5), (4, .5), (3, .5), (4, 1), (2, .5), (0, 1)],
        [(2, .5), (3, .5), (4, .5), (5, 1), (4, .5), (3, 1)],
        [(4, .5), (4, .5), (3, .5), (4, 1), (7, .5), (6, 1)],
        [(5, .5), (4, .5), (3, .5), (2, 1.5), (None, 1)],
    ]
    for b, bar in enumerate(verse * 2):
        melody(bar, t + b * 4, fn=horn_brass, volume=0.32)
    t += 32
    # Chorus (8 bars): soaring, an octave up, over brighter chords
    power_chords([5, 6, 0, 4, 5, 6, 2, 4], t, 8)
    rock_beat(t, 8)
    chorus = [
        [(7, 1.5), (6, .5), (7, 1), (9, 1)],
        [(8, 1), (7, 1), (6, 1), (4, 1)],
        [(5, 1.5), (6, .5), (7, 1), (9, 1)],
        [(11, 2), (9, 1), (8, 1)],
        [(7, 1.5), (6, .5), (7, 1), (9, 1)],
        [(8, 1), (7, 1), (6, 1), (8, 1)],
        [(9, 1.5), (8, .5), (7, 1), (6, 1)],
        [(7, 4)],
    ]
    for b, bar in enumerate(chorus):
        melody(bar, t + b * 4, volume=0.36)
        melody(bar, t + b * 4, fn=strings_note, volume=0.12, octave=-1)
    t += 32
    # Final hit: a big chord and crash for the title
    for f in (0, 4, 7):
        add(brass(midi_freq(key - 12 + degree_semis(scale, 0) + (3 if f == 4 else f)), beat * 6, 0.22), t)
    add(kick(0.6, 120, 35, 1.0, curve=1.6), t)
    add(crash(0.3, 3.0), t)
    return gba(buf, gain=0.92)


def brass_chord(freqs, length, volume):
    out = None
    for f in freqs:
        v = fm_voice(f, length, volume, 1.0, 3.2, attack=0.003, decay=0.06, sustain=0.6, release=0.02, index_hold=0.9)
        out = v if out is None else [a + b for a, b in zip(out, v)]
    return out


def strings_note(freq, length, volume):
    return strings([freq], length, volume)


def title_spec():
    """The title / menu theme (written out note for note)."""
    names = [
        "G4:.5 C5:.5 E5:1 D5:.5 C5:.5 D5:1", "B4:1 D5:.5 G5:1.5 F5:.5 E5:.5", "E5:1 C5:.5 A4:.5 C5:1 E5:1",
        "F5:1.5 E5:.5 D5:1 C5:1", "G4:.5 C5:.5 E5:1 G5:.5 A5:.5 G5:1", "F5:.5 E5:.5 D5:1 B4:1 G4:1",
        "A4:.5 C5:.5 F5:1 E5:.5 D5:.5 C5:1", "D5:1.5 E5:.5 D5:2", "A5:1 G5:.5 E5:.5 C5:1 E5:1",
        "F5:1 E5:.5 C5:.5 A4:2", "G5:1 E5:.5 G5:.5 C6:1.5 B5:.5", "A5:.5 G5:.5 F5:.5 E5:.5 D5:2",
        "E5:.5 F5:.5 G5:1 A5:1 C6:1", "B5:.5 A5:.5 F5:1 A5:1 G5:1", "F5:1 E5:.5 D5:.5 A4:1 D5:1",
        "D5:.5 E5:.5 F5:.5 G5:.5 B4:1 D5:1",
    ]
    key = note_midi("C5")
    major = SCALES["major"]
    melody = []
    for bar in names:
        for tok in bar.split():
            n, l = tok.split(":")
            semis = note_midi(n) - key
            # Back to a scale degree so the generic renderer can play it
            octave, rem = divmod(semis, 12)
            melody.append((octave * 7 + major.index(rem), float(l)))
    return dict(style="western", key=key, bpm=132, melody=melody,
                progression=[0, 4, 5, 3, 0, 4, 3, 4, 5, 3, 0, 4, 5, 3, 1, 4])


def encode(wav_path, out_path):
    """WAV -> mono OGG Vorbis with ffmpeg's built-in encoder (Bevy plays Vorbis)."""
    subprocess.run(["ffmpeg", "-y", "-loglevel", "error", "-i", str(wav_path),
                    "-c:a", "vorbis", "-strict", "-2", "-ac", "2", "-b:a", "96k", str(out_path)], check=True)


def main():
    pieces = all_pieces()
    names = sys.argv[1:] or list(pieces)
    OUT.mkdir(parents=True, exist_ok=True)
    for name in names:
        data = pieces[name]()
        with tempfile.TemporaryDirectory() as tmp:
            wav = Path(tmp) / f"{name}.wav"
            write_wav(wav, data)
            if shutil.which("ffmpeg"):
                encode(wav, OUT / f"{name}.ogg")
                print(f"{name}: {len(data) / RATE:.1f}s")
            else:
                shutil.copy(wav, OUT / f"{name}.wav")
                print(f"ffmpeg not found; wrote {name}.wav")


if __name__ == "__main__":
    main()
