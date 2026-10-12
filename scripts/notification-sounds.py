#!/usr/bin/env python3
"""Synthesize Kodem's notification sounds into assets/sounds, or report their levels."""
import argparse
import math
from pathlib import Path
import struct
import wave

ROOT = Path(__file__).resolve().parents[1]
SOUNDS = ROOT / "assets/sounds"
RATE = 44100
# Peak level of every sound. System alert sounds sit close to full scale; a quieter file is
# easy to miss at a normal alert volume.
PEAK = 10 ** (-1.0 / 20)


def strike(buffer, start, frequency, partials, level=1.0, attack=0.004):
    """Add one struck note: (ratio, amplitude, decay seconds) partials with a soft attack."""
    first = int(start * RATE)
    for ratio, amplitude, decay in partials:
        step = 2 * math.pi * frequency * ratio / RATE
        # A partial is silent once it is 80 dB below its start.
        length = min(len(buffer) - first, int(decay * 9.2 * RATE))
        for index in range(length):
            time = index / RATE
            rise = 0.5 - 0.5 * math.cos(math.pi * min(time / attack, 1.0))
            buffer[first + index] += level * amplitude * rise * math.exp(-time / decay) * math.sin(step * index)


def pulse(buffer, start, frequency, harmonics, hold, release, level=1.0):
    """Add one rounded synthetic pulse: a short held tone with a fast release."""
    first = int(start * RATE)
    attack = 0.005
    length = min(len(buffer) - first, int((attack + hold + release * 7) * RATE))
    for index in range(length):
        time = index / RATE
        if time < attack:
            envelope = 0.5 - 0.5 * math.cos(math.pi * time / attack)
        elif time < attack + hold:
            envelope = 1.0
        else:
            envelope = math.exp(-(time - attack - hold) / release)
        sample = sum(amplitude * math.sin(2 * math.pi * frequency * harmonic * time) for harmonic, amplitude in harmonics)
        buffer[first + index] += level * envelope * sample


def room(buffer, wet):
    """Mix in a short damped Schroeder reverb so a sound does not end dry."""
    tail = [0.0] * len(buffer)
    for delay, feedback in [(1557, 0.66), (1617, 0.65), (1491, 0.67), (1422, 0.68)]:
        line = [0.0] * delay
        damped = 0.0
        for index, sample in enumerate(buffer):
            out = line[index % delay]
            damped = out * 0.7 + damped * 0.3
            line[index % delay] = sample + damped * feedback
            tail[index] += out * 0.25
    for delay in [225, 556]:
        line = [0.0] * delay
        for index, sample in enumerate(tail):
            out = line[index % delay]
            line[index % delay] = sample + out * 0.5
            tail[index] = out - sample * 0.5
    return [dry + wet * reverb for dry, reverb in zip(buffer, tail)]


def chime():
    """Two warm mallet notes that rise a fourth: the default, friendly and clear."""
    buffer = [0.0] * int(1.6 * RATE)
    mallet = [(1, 1.0, 0.34), (2, 0.42, 0.22), (3, 0.16, 0.13), (4.2, 0.1, 0.07)]
    strike(buffer, 0.0, 783.99, mallet, 0.85)
    strike(buffer, 0.17, 1046.50, mallet, 1.0)
    return room(buffer, 0.16)


def bell():
    """One bright bell strike with inharmonic partials and a slow shimmer."""
    buffer = [0.0] * int(2.0 * RATE)
    partials = [
        (0.5, 0.22, 0.6), (1.0, 1.0, 0.5), (1.004, 0.45, 0.48), (1.2, 0.42, 0.4), (1.5, 0.3, 0.34),
        (2.0, 0.55, 0.3), (2.008, 0.25, 0.29), (2.5, 0.2, 0.22), (3.0, 0.26, 0.18), (4.1, 0.12, 0.11),
    ]
    strike(buffer, 0.0, 880.0, partials, 1.0, attack=0.002)
    return room(buffer, 0.14)


def glass():
    """Two quick, crisp taps on glass: high, short and light."""
    buffer = [0.0] * int(1.0 * RATE)
    bar = [(1, 1.0, 0.2), (2.756, 0.34, 0.09), (5.404, 0.12, 0.04)]
    strike(buffer, 0.0, 1567.98, bar, 0.8, attack=0.0015)
    strike(buffer, 0.115, 2093.0, bar, 1.0, attack=0.0015)
    return room(buffer, 0.2)


def pulse_pair():
    """Two short synthetic pulses on one note: compact and hard to mistake for music."""
    buffer = [0.0] * int(0.6 * RATE)
    harmonics = [(1, 1.0), (2, 0.3), (3, 0.12)]
    pulse(buffer, 0.0, 880.0, harmonics, 0.045, 0.03, 0.85)
    pulse(buffer, 0.15, 880.0, harmonics, 0.06, 0.045, 1.0)
    return room(buffer, 0.1)


def ascend():
    """A fast rising major arpeggio that lands on the octave: a clear "done"."""
    buffer = [0.0] * int(1.7 * RATE)
    pluck = [(1, 1.0, 0.24), (2, 0.38, 0.15), (3, 0.18, 0.09), (4, 0.08, 0.05)]
    ring = [(1, 1.0, 0.36), (2, 0.38, 0.24), (3, 0.18, 0.14), (4, 0.08, 0.08)]
    for index, frequency in enumerate([523.25, 659.26, 783.99]):
        strike(buffer, index * 0.085, frequency, pluck, 0.7 + index * 0.1)
    strike(buffer, 0.255, 1046.50, ring, 1.0)
    return room(buffer, 0.16)


DESIGNS = {"chime": chime, "bell": bell, "glass": glass, "pulse": pulse_pair, "ascend": ascend}


def master(buffer):
    """Normalize to the shared peak, fade the end to silence and return 16-bit samples."""
    fade = int(0.1 * RATE)
    for index in range(fade):
        buffer[-1 - index] *= 0.5 - 0.5 * math.cos(math.pi * index / fade)
    scale = PEAK / max(abs(sample) for sample in buffer)
    return [round(sample * scale * 32767) for sample in buffer]


def levels(path):
    """Describe a sound: duration, peak and the level of its loudest 200 ms."""
    with wave.open(str(path)) as audio:
        frames = audio.getnframes()
        samples = struct.unpack(f"<{frames * audio.getnchannels()}h", audio.readframes(frames))
        rate = audio.getframerate()
    window = int(rate * 0.2)
    energy = loudest = sum(sample * sample for sample in samples[:window])
    for index in range(window, len(samples)):
        energy += samples[index] ** 2 - samples[index - window] ** 2
        loudest = max(loudest, energy)
    peak = 20 * math.log10(max(abs(sample) for sample in samples) / 32768)
    body = 20 * math.log10(math.sqrt(loudest / window) / 32768)
    return f"{path.name}: {frames / rate:.2f} s, peak {peak:.1f} dBFS, loudest 200 ms {body:.1f} dBFS, ends at {samples[-1]}"


def main():
    """Write every sound, or only report the levels of the tracked files."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=["write", "levels"])
    command = parser.parse_args().command
    for name, design in DESIGNS.items():
        path = SOUNDS / f"kodem-{name}.wav"
        if command == "write":
            samples = master(design())
            with wave.open(str(path), "wb") as audio:
                audio.setnchannels(1)
                audio.setsampwidth(2)
                audio.setframerate(RATE)
                audio.writeframes(struct.pack(f"<{len(samples)}h", *samples))
        print(levels(path))


if __name__ == "__main__":
    main()
