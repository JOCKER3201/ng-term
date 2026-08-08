#!/usr/bin/env python3
"""ng-term sci-fi sound theme — procedural synthesis.

Generates the whole UI sound set as 48 kHz stereo 16-bit WAV files.
Everything is deterministic: same script, same bytes.
"""

import os
import struct
import sys
import wave

import numpy as np

SR = 48000
RNG = np.random.default_rng(20260808)


# ---------------------------------------------------------------- primitives

def n_of(dur):
    return int(round(dur * SR))


def t_of(dur):
    return np.arange(n_of(dur), dtype=np.float64) / SR


def silence(dur):
    return np.zeros(n_of(dur))


def env(dur, a=0.005, d=None, s=0.0, r=0.05, curve=2.0):
    """Attack / decay / sustain / release envelope."""
    n = n_of(dur)
    na = min(n_of(a), n)
    nr = min(n_of(r), n - na)
    nd = n_of(d) if d is not None else max(0, n - na - nr)
    nd = min(nd, n - na - nr)
    ns = max(0, n - na - nd - nr)
    parts = []
    if na:
        parts.append(np.linspace(0.0, 1.0, na) ** (1.0 / curve))
    if nd:
        parts.append(s + (1.0 - s) * (np.linspace(1.0, 0.0, nd) ** curve))
    if ns:
        parts.append(np.full(ns, s))
    if nr:
        start = s if ns or nd else 1.0
        parts.append(start * (np.linspace(1.0, 0.0, nr) ** curve))
    e = np.concatenate(parts) if parts else np.zeros(n)
    return np.resize(e, n)


def perc(dur, decay=None, curve=3.0, a=0.002):
    """Percussive envelope: fast attack, exponential-ish tail."""
    return env(dur, a=a, d=decay or (dur - a), s=0.0, r=0.0, curve=curve)


def phase(freq, dur):
    """Phase ramp for a constant or per-sample frequency."""
    n = n_of(dur)
    f = np.full(n, float(freq)) if np.isscalar(freq) else np.resize(freq, n)
    return 2 * np.pi * np.cumsum(f) / SR


def sweep(f0, f1, dur, curve=1.0):
    """Frequency ramp; curve > 1 falls fast then settles."""
    x = np.linspace(0.0, 1.0, n_of(dur)) ** curve
    return f0 + (f1 - f0) * x


def sine(freq, dur, ph=0.0):
    return np.sin(phase(freq, dur) + ph)


def tri(freq, dur):
    p = (phase(freq, dur) / (2 * np.pi)) % 1.0
    return 4.0 * np.abs(p - 0.5) - 1.0


def saw(freq, dur):
    p = (phase(freq, dur) / (2 * np.pi)) % 1.0
    return 2.0 * p - 1.0


def square(freq, dur, pw=0.5):
    p = (phase(freq, dur) / (2 * np.pi)) % 1.0
    return np.where(p < pw, 1.0, -1.0)


def fm(carrier, ratio, index, dur):
    """Simple 2-operator FM — the backbone of every metallic bleep here."""
    n = n_of(dur)
    c = np.full(n, float(carrier)) if np.isscalar(carrier) else np.resize(carrier, n)
    m = c * ratio
    idx = np.full(n, float(index)) if np.isscalar(index) else np.resize(index, n)
    mod = np.sin(2 * np.pi * np.cumsum(m) / SR) * idx
    return np.sin(2 * np.pi * np.cumsum(c) / SR + mod)


def noise(dur):
    return RNG.uniform(-1.0, 1.0, n_of(dur))


# ------------------------------------------------------------------- filters

def _biquad(x, b0, b1, b2, a1, a2):
    y = np.empty_like(x)
    x1 = x2 = y1 = y2 = 0.0
    for i in range(x.size):
        xi = x[i]
        yi = b0 * xi + b1 * x1 + b2 * x2 - a1 * y1 - a2 * y2
        y[i] = yi
        x2, x1 = x1, xi
        y2, y1 = y1, yi
    return y


def _coeffs(kind, f0, q):
    f0 = max(20.0, min(f0, SR * 0.45))
    w = 2 * np.pi * f0 / SR
    alpha = np.sin(w) / (2 * q)
    cw = np.cos(w)
    a0 = 1 + alpha
    if kind == "lp":
        b = ((1 - cw) / 2, 1 - cw, (1 - cw) / 2)
    elif kind == "hp":
        b = ((1 + cw) / 2, -(1 + cw), (1 + cw) / 2)
    else:  # bp (constant skirt)
        b = (alpha, 0.0, -alpha)
    a = (-2 * cw, 1 - alpha)
    return b[0] / a0, b[1] / a0, b[2] / a0, a[0] / a0, a[1] / a0


def filt(x, kind, f0, q=0.707):
    """Static biquad. For sweeps use `filt_sweep`."""
    return _biquad(x, *_coeffs(kind, f0, q))


def filt_sweep(x, kind, f_curve, q=0.707):
    """Time-varying biquad — coefficients recomputed in blocks of 64."""
    f = np.resize(np.asarray(f_curve, dtype=float), x.size)
    y = np.empty_like(x)
    x1 = x2 = y1 = y2 = 0.0
    blk = 64
    for start in range(0, x.size, blk):
        end = min(start + blk, x.size)
        b0, b1, b2, a1, a2 = _coeffs(kind, float(f[start]), q)
        for i in range(start, end):
            xi = x[i]
            yi = b0 * xi + b1 * x1 + b2 * x2 - a1 * y1 - a2 * y2
            y[i] = yi
            x2, x1 = x1, xi
            y2, y1 = y1, yi
    return y


# -------------------------------------------------------------------- shapes

def bitcrush(x, bits=8, hold=1):
    """Digital grit: quantise amplitude, optionally hold samples."""
    if hold > 1:
        x = np.repeat(x[::hold], hold)[: x.size]
        if x.size < hold:
            x = np.resize(x, x.size)
    steps = 2 ** (bits - 1)
    return np.round(x * steps) / steps


def drive(x, amount=3.0):
    return np.tanh(x * amount) / np.tanh(amount)


def delay(x, time, feedback=0.35, mix=0.35, taps=6):
    d = n_of(time)
    if d <= 0:
        return x
    out = x.copy()
    out = np.pad(out, (0, d * taps))
    g = feedback
    for k in range(1, taps + 1):
        out[d * k : d * k + x.size] += x * g * mix
        g *= feedback
    return out


def reverb(x, room=0.82, damp=0.35, mix=0.3, tail=0.9):
    """Schroeder reverb — 4 combs into 2 allpasses."""
    pad = np.pad(x, (0, n_of(tail)))
    combs = [1116, 1188, 1277, 1356]
    acc = np.zeros_like(pad)
    for c in combs:
        buf = np.zeros(c)
        y = np.empty_like(pad)
        i = 0
        lp = 0.0
        for s in range(pad.size):
            v = buf[i]
            lp = v * (1 - damp) + lp * damp
            y[s] = v
            buf[i] = pad[s] + lp * room
            i = (i + 1) % c
        acc += y
    acc /= len(combs)
    for a, gain in ((556, 0.5), (441, 0.5)):
        buf = np.zeros(a)
        y = np.empty_like(acc)
        i = 0
        for s in range(acc.size):
            v = buf[i]
            out = -acc[s] + v
            buf[i] = acc[s] + v * gain
            y[s] = out
            i = (i + 1) % a
        acc = y
    return np.pad(x, (0, n_of(tail))) * (1 - mix) + acc * mix


def stereo(x, spread_ms=6.0, detune=0.0):
    """Mono -> stereo via Haas offset (and optional per-side amplitude tilt)."""
    d = n_of(spread_ms / 1000.0)
    left = np.pad(x, (0, d))
    right = np.pad(x, (d, 0))
    if detune:
        left = left * (1.0 + detune)
        right = right * (1.0 - detune)
    return np.stack([left, right], axis=1)


def mix_at(base, part, at):
    """Add `part` into `base` starting at time `at` (seconds), growing as needed."""
    i = n_of(at)
    need = i + part.size
    if base.size < need:
        base = np.pad(base, (0, need - base.size))
    base[i : i + part.size] += part
    return base


def norm(x, peak=0.89):
    m = np.max(np.abs(x))
    return x if m < 1e-9 else x * (peak / m)


def fade_edges(x, ms=3.0):
    n = min(n_of(ms / 1000.0), x.size // 2)
    if n <= 0:
        return x
    ramp = np.linspace(0.0, 1.0, n)
    if x.ndim == 2:
        ramp = ramp[:, None]
    x = x.copy()
    x[:n] *= ramp
    x[-n:] *= ramp[::-1]
    return x


def dc_block(x):
    """One-pole DC blocker — asymmetric pulse waves leave a real offset."""
    a = 0.9995
    y = np.empty_like(x)
    x1 = y1 = 0.0
    for i in range(x.size):
        y1 = x[i] - x1 + a * y1
        x1 = x[i]
        y[i] = y1
    return y


def write_wav(path, data, peak=0.89, fade=True):
    """Mono 16-bit WAV — the audio layer pans these at playback time, so
    storing two identical channels would only double the size."""
    if data.ndim == 2:
        data = data.mean(axis=1)
    data = dc_block(data)
    data = data / max(1e-9, np.max(np.abs(data))) * peak
    # A looping bed must not be faded — the seam has to stay continuous.
    if fade:
        data = fade_edges(data, 2.0)
    pcm = np.clip(data, -1.0, 1.0)
    pcm = (pcm * 32767.0).astype("<i2")
    with wave.open(path, "wb") as w:
        w.setnchannels(1)
        w.setsampwidth(2)
        w.setframerate(SR)
        w.writeframes(pcm.tobytes())
    return path


# ---------------------------------------------------------------- the sounds
#
# Palette notes: the theme leans on FM bleeps around 700-2600 Hz, resonant
# filtered noise for "air", and short bright tails. Nothing is longer than it
# has to be — UI sounds that outstay their welcome become fatiguing fast.


def s_key(seed_shift=0.0):
    """Terminal keypress — dry, tiny, slightly different each variant.

    Keys fire hundreds of times a minute, so the energy has to sit in the
    mids: anything treble-heavy turns into hiss after a paragraph of typing.
    """
    dur = 0.055
    base = 1050 * (2 ** (seed_shift / 12.0))
    body = fm(sweep(base * 1.7, base, dur, 2.6), 2.01, sweep(2.4, 0.3, dur, 2.0), dur)
    body *= perc(dur, decay=0.05, curve=4.0, a=0.0016)
    tick = filt(noise(0.010), "bp", 2600, 1.1) * perc(0.010, curve=5.0, a=0.001) * 0.35
    out = mix_at(body.copy(), tick, 0.0)
    # Two cascaded poles — a single biquad is too gentle to tame the transient.
    out = filt(filt(out, "lp", 4600, 0.7), "lp", 4600, 0.7)
    return norm(out, 0.7)


def s_key_return():
    """Enter — a touch lower and rounder than the plain keys."""
    dur = 0.11
    body = fm(sweep(820, 560, dur, 2.2), 1.5, sweep(2.6, 0.4, dur, 2.4), dur)
    body *= perc(dur, decay=0.1, curve=3.2, a=0.001)
    sub = sine(sweep(300, 220, dur, 2.0), dur) * perc(dur, curve=2.6) * 0.45
    out = filt(norm(body + sub, 0.8), "lp", 4800, 0.8)
    return norm(reverb(out, room=0.6, mix=0.14, tail=0.18), 0.8)


def s_key_erase():
    """Backspace — reversed feel, pitch bends up then cuts."""
    dur = 0.07
    body = fm(sweep(620, 1050, dur, 0.6), 2.01, sweep(0.8, 2.2, dur, 0.7), dur)
    body *= env(dur, a=0.03, d=0.0, s=1.0, r=0.038, curve=2.4)
    return norm(filt(body, "lp", 4600, 0.8), 0.72)


def s_hover():
    """Focus/hover — must be almost subliminal."""
    dur = 0.045
    body = sine(sweep(2400, 3100, dur, 0.8), dur) * perc(dur, curve=3.0, a=0.004)
    air = filt(noise(dur), "hp", 6000) * perc(dur, curve=4.0) * 0.25
    return norm(body + air, 0.34)


def s_click():
    """Primary UI button — confident, two-tone."""
    dur = 0.13
    a = fm(sweep(1500, 1150, dur, 2.0), 2.0, sweep(2.2, 0.4, dur, 2.5), dur)
    a *= perc(dur, decay=0.06, curve=3.4, a=0.0008)
    b = fm(2300, 1.5, 1.2, 0.05) * perc(0.05, curve=4.0, a=0.0006) * 0.4
    out = mix_at(a.copy(), b, 0.028)
    tick = filt(noise(0.01), "bp", 3000, 1.0) * perc(0.01, curve=5.0) * 0.28
    out = filt(mix_at(out, tick, 0.0), "lp", 6000, 0.8)
    return norm(reverb(out, room=0.62, mix=0.16, tail=0.22), 0.85)


def s_toggle_on():
    dur = 0.16
    up = fm(sweep(1100, 2200, dur, 0.55), 2.0, sweep(2.0, 0.8, dur, 1.5), dur)
    up *= env(dur, a=0.004, d=0.02, s=0.55, r=0.11, curve=2.2)
    spark = filt(noise(0.03), "bp", 7000, 2.0) * perc(0.03, curve=4.0) * 0.3
    out = mix_at(up.copy(), spark, 0.09)
    return norm(reverb(out, room=0.66, mix=0.2, tail=0.28), 0.8)


def s_toggle_off():
    dur = 0.14
    dn = fm(sweep(1900, 820, dur, 1.8), 2.0, sweep(2.5, 0.5, dur, 2.0), dur)
    dn *= env(dur, a=0.003, d=0.02, s=0.5, r=0.1, curve=2.6)
    return norm(reverb(dn, room=0.6, mix=0.16, tail=0.24), 0.78)


def s_panel_open():
    """Settings window sliding in — rising filtered noise + a chord stab."""
    dur = 0.5
    air = filt_sweep(noise(dur), "bp", sweep(400, 4200, dur, 0.7), q=1.4)
    air *= env(dur, a=0.18, d=0.06, s=0.5, r=0.24, curve=2.0) * 0.55
    chord = np.zeros(n_of(dur))
    for i, f in enumerate((523.25, 783.99, 1046.5)):
        v = fm(f, 2.0, 1.4, dur) * env(dur, a=0.02 + i * 0.03, d=0.1, s=0.35, r=0.3, curve=2.2)
        chord += v / (i + 1.6)
    out = norm(air, 0.6) + norm(chord, 0.7)
    return norm(reverb(out, room=0.78, mix=0.28, tail=0.5), 0.85)


def s_panel_close():
    dur = 0.34
    air = filt_sweep(noise(dur), "bp", sweep(3800, 500, dur, 1.6), q=1.4)
    air *= env(dur, a=0.01, d=0.08, s=0.4, r=0.24, curve=2.4) * 0.6
    tone = fm(sweep(1200, 480, dur, 2.0), 1.5, sweep(2.0, 0.3, dur, 2.0), dur)
    tone *= env(dur, a=0.004, d=0.06, s=0.3, r=0.26, curve=2.4)
    out = norm(air, 0.6) + norm(tone, 0.7)
    return norm(reverb(out, room=0.7, mix=0.22, tail=0.35), 0.82)


def s_grab():
    """Picking a widget up in the layout editor — magnetic latch."""
    dur = 0.1
    body = fm(sweep(520, 880, dur, 0.5), 3.0, sweep(0.8, 3.0, dur, 0.8), dur)
    body *= env(dur, a=0.012, d=0.02, s=0.5, r=0.06, curve=2.0)
    return norm(drive(body, 2.0), 0.7)


def s_drop():
    dur = 0.14
    thud = sine(sweep(420, 150, dur, 2.4), dur) * perc(dur, curve=2.8, a=0.002)
    click = filt(noise(0.02), "bp", 2600, 1.4) * perc(0.02, curve=4.0) * 0.5
    out = mix_at(thud.copy(), click, 0.0)
    return norm(drive(out, 1.8), 0.78)


def s_snap():
    """Grid snap — short, crystalline, obviously 'locked in'."""
    dur = 0.07
    a = fm(3200, 2.0, 1.6, dur) * perc(dur, curve=4.5, a=0.0006)
    b = fm(4800, 1.5, 1.0, 0.035) * perc(0.035, curve=5.0) * 0.45
    out = mix_at(a.copy(), b, 0.012)
    return norm(reverb(out, room=0.55, mix=0.18, tail=0.16), 0.6)


def s_save():
    """Layout written — a small rising three-note confirmation."""
    out = np.zeros(1)
    for i, f in enumerate((880.0, 1174.7, 1567.98)):
        v = fm(f, 2.0, sweep(2.5, 0.4, 0.2, 2.0), 0.2)
        v *= env(0.2, a=0.004, d=0.05, s=0.35, r=0.14, curve=2.4)
        out = mix_at(out, v * (0.9 - i * 0.12), i * 0.075)
    return norm(reverb(out, room=0.75, mix=0.26, tail=0.45), 0.85)


def s_error():
    """Denied — dissonant, buzzy, unmistakably negative."""
    dur = 0.3
    a = square(sweep(320, 260, dur, 1.4), dur, pw=0.42)
    b = square(sweep(452, 368, dur, 1.4), dur, pw=0.38)
    body = (a + b * 0.9) * env(dur, a=0.002, d=0.05, s=0.55, r=0.2, curve=2.2)
    body = filt_sweep(body, "lp", sweep(3000, 900, dur, 1.5), q=1.1)
    body = bitcrush(body, bits=6)
    return norm(reverb(drive(body, 2.2), room=0.6, mix=0.16, tail=0.3), 0.8)


def s_alert():
    """Notification — two urgent pips, bright but not harsh."""
    out = np.zeros(1)
    for i in range(2):
        v = fm(1568.0, 2.0, sweep(1.8, 0.4, 0.09, 2.0), 0.09)
        v *= env(0.09, a=0.003, d=0.03, s=0.3, r=0.055, curve=2.6)
        out = mix_at(out, v, i * 0.135)
    out = filt(out, "lp", 7000, 0.8)
    return norm(reverb(out, room=0.7, mix=0.22, tail=0.35), 0.82)


def s_theme():
    """Theme / mode change — a wide sweep across the spectrum."""
    dur = 0.6
    air = filt_sweep(noise(dur), "bp", sweep(300, 8000, dur, 0.55), q=2.2)
    air *= env(dur, a=0.22, d=0.1, s=0.4, r=0.28, curve=2.0)
    tone = fm(sweep(220, 1760, dur, 0.6), 2.0, sweep(0.6, 3.2, dur, 0.8), dur)
    tone *= env(dur, a=0.2, d=0.1, s=0.4, r=0.3, curve=2.0)
    out = norm(air, 0.55) + norm(tone, 0.6)
    return norm(reverb(out, room=0.84, mix=0.34, tail=0.6), 0.86)


def s_boot():
    """Startup sequence — the signature sound of the whole theme."""
    out = np.zeros(n_of(0.1))

    # 1. Power-up rumble: a sub sweeping up under everything.
    dur = 1.6
    sub = sine(sweep(28, 96, dur, 0.7), dur) * env(dur, a=0.5, d=0.3, s=0.5, r=0.7, curve=1.8)
    out = mix_at(out, norm(sub, 0.55), 0.0)

    # 2. Capacitor whine climbing to pitch.
    dur = 1.25
    whine = saw(sweep(90, 1240, dur, 0.42), dur)
    whine = filt_sweep(whine, "lp", sweep(700, 6500, dur, 0.5), q=2.6)
    whine *= env(dur, a=0.55, d=0.2, s=0.55, r=0.45, curve=2.0)
    out = mix_at(out, norm(whine, 0.32), 0.12)

    # 3. Data chatter: quiet bit-crushed bleeps, like a self-test scrolling by.
    chatter = np.zeros(n_of(1.0))
    for k in range(14):
        f = 1500 + (k * 313 % 1900)
        v = fm(f, 2.0, 2.0, 0.026) * perc(0.026, curve=4.0, a=0.001)
        chatter = mix_at(chatter, bitcrush(v, bits=5) * 0.35, 0.05 + k * 0.062)
    out = mix_at(out, norm(chatter, 0.3), 0.35)

    # 4. The arrival: a bright fifth landing when the whine tops out.
    land = np.zeros(1)
    for i, f in enumerate((523.25, 783.99, 1567.98)):
        v = fm(f, 2.0, sweep(3.2, 0.5, 0.9, 2.0), 0.9)
        v *= env(0.9, a=0.006, d=0.22, s=0.34, r=0.62, curve=2.2)
        land = mix_at(land, v * (0.95 - i * 0.16), i * 0.018)
    out = mix_at(out, norm(land, 0.8), 1.32)

    # 5. Trailing shimmer.
    shim = filt(noise(0.8), "hp", 6500) * env(0.8, a=0.05, d=0.2, s=0.25, r=0.55, curve=2.4)
    out = mix_at(out, norm(shim, 0.18), 1.38)

    return norm(reverb(out, room=0.86, mix=0.3, tail=1.1), 0.92)


def s_shutdown():
    """Power-down — the boot sound running backwards, conceptually."""
    dur = 1.1
    whine = saw(sweep(1100, 60, dur, 1.8), dur)
    whine = filt_sweep(whine, "lp", sweep(6000, 320, dur, 1.6), q=2.4)
    whine *= env(dur, a=0.01, d=0.25, s=0.5, r=0.8, curve=2.2)
    sub = sine(sweep(90, 24, dur, 1.6), dur) * env(dur, a=0.02, d=0.3, s=0.45, r=0.75, curve=2.0)
    stab = fm(392.0, 2.0, sweep(2.6, 0.3, 0.5, 2.0), 0.5)
    stab *= env(0.5, a=0.004, d=0.14, s=0.25, r=0.34, curve=2.4)
    out = mix_at(norm(whine, 0.5) + norm(sub, 0.5), norm(stab, 0.65), 0.0)
    return norm(reverb(out, room=0.8, mix=0.28, tail=0.7), 0.88)


def s_ambient():
    """Seamless 8 s background hum — optional, very quiet by design."""
    dur = 8.0
    n = n_of(dur)
    out = np.zeros(n)
    # Detuned drone partials, all integer cycles so the loop is seamless.
    for f_target, amp in ((55.0, 1.0), (82.5, 0.45), (110.0, 0.3), (164.5, 0.12)):
        cycles = round(f_target * dur)
        f = cycles / dur
        out += np.sin(2 * np.pi * f * np.arange(n) / SR) * amp
    # Slow filtered-noise wash, wrapped so the ends meet.
    wash = filt(noise(dur), "bp", 900, 0.8)
    lfo_cycles = 3
    lfo = 0.5 + 0.5 * np.sin(2 * np.pi * lfo_cycles * np.arange(n) / n)
    out += wash * lfo * 0.16
    xf = n_of(0.35)
    ramp = np.linspace(0.0, 1.0, xf)
    head, tail = out[:xf].copy(), out[-xf:].copy()
    out[:xf] = head * ramp + tail * (1 - ramp)
    out = out[:-xf]
    return norm(out, 0.3)


SOUNDS = {
    "boot": s_boot,
    "shutdown": s_shutdown,
    "key1": lambda: s_key(0.0),
    "key2": lambda: s_key(1.0),
    "key3": lambda: s_key(-1.5),
    "key4": lambda: s_key(2.5),
    "key-return": s_key_return,
    "key-erase": s_key_erase,
    "hover": s_hover,
    "click": s_click,
    "toggle-on": s_toggle_on,
    "toggle-off": s_toggle_off,
    "panel-open": s_panel_open,
    "panel-close": s_panel_close,
    "grab": s_grab,
    "drop": s_drop,
    "snap": s_snap,
    "save": s_save,
    "error": s_error,
    "alert": s_alert,
    "theme": s_theme,
    "ambient": s_ambient,
}

# Peak levels per sound, so the set is balanced when played in the app.
LEVELS = {
    "hover": 0.30,
    "key1": 0.55, "key2": 0.55, "key3": 0.55, "key4": 0.55,
    "key-return": 0.62, "key-erase": 0.58,
    "snap": 0.55, "grab": 0.6, "drop": 0.66,
    "click": 0.72, "toggle-on": 0.7, "toggle-off": 0.7,
    "panel-open": 0.75, "panel-close": 0.72,
    "save": 0.78, "alert": 0.8, "error": 0.8, "theme": 0.78,
    "boot": 0.92, "shutdown": 0.88,
    "ambient": 0.22,
}


def main():
    out_dir = sys.argv[1] if len(sys.argv) > 1 else "sfx"
    os.makedirs(out_dir, exist_ok=True)
    names = sys.argv[2:] or list(SOUNDS)
    made = []
    for name in names:
        data = SOUNDS[name]()
        path = os.path.join(out_dir, f"{name}.wav")
        write_wav(path, data, peak=LEVELS.get(name, 0.85), fade=(name != "ambient"))
        made.append((name, path, os.path.getsize(path), data.shape[0] / SR))
        print(f"  {name:<12} {data.shape[0] / SR:5.2f}s  {os.path.getsize(path) / 1024:6.1f} kB")

    # Audition file: every sound in order, 0.45 s apart (ambient excluded).
    demo = np.zeros(n_of(0.2))
    at = 0.2
    for name in names:
        if name == "ambient":
            continue
        with wave.open(os.path.join(out_dir, f"{name}.wav"), "rb") as w:
            seg = np.frombuffer(w.readframes(w.getnframes()), dtype="<i2")
        seg = seg.astype(np.float64) / 32767.0
        i = n_of(at)
        if demo.size < i + seg.size:
            demo = np.pad(demo, (0, i + seg.size - demo.size))
        demo[i : i + seg.size] += seg
        at += seg.size / SR + 0.45
    write_wav(os.path.join(out_dir, "_audition.wav"), demo, peak=0.92)
    print(f"\n  audition -> {os.path.join(out_dir, '_audition.wav')} ({at:.1f}s)")


if __name__ == "__main__":
    main()
