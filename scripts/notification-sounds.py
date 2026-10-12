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


def rise(time, attack):
    """Return the soft start of a note: a raised cosine from 0 to 1 over `attack` seconds."""
    return 0.5 - 0.5 * math.cos(math.pi * min(time / attack, 1.0))


def strike(buffer, start, frequency, partials, level=1.0, attack=0.006):
    """Add one struck note: (ratio, amplitude, decay seconds) partials with a soft attack."""
    first = int(start * RATE)
    for ratio, amplitude, decay in partials:
        step = 2 * math.pi * frequency * ratio / RATE
        # A partial is silent once it is 80 dB below its start.
        length = min(len(buffer) - first, int(decay * 9.2 * RATE))
        for index in range(length):
            time = index / RATE
            buffer[first + index] += level * amplitude * rise(time, attack) * math.exp(-time / decay) * math.sin(step * index)


def tine(buffer, start, frequency, decay, brightness, level=1.0, attack=0.006):
    """Add one electric-piano note: its overtones start at `brightness` and fade before its body.

    The tone modulates its own phase, so the overtones are harmonic and leave a pure tone behind.
    """
    first = int(start * RATE)
    step = 2 * math.pi * frequency / RATE
    length = min(len(buffer) - first, int(decay * 9.2 * RATE))
    for index in range(length):
        time = index / RATE
        phase = step * index
        overtones = brightness * math.exp(-time / (decay * 0.3))
        envelope = level * rise(time, attack) * math.exp(-time / decay)
        buffer[first + index] += envelope * math.sin(phase + overtones * math.sin(phase))


def pop(buffer, start, frequency, decay, bend, level=1.0, attack=0.008):
    """Add one rounded pop: a short tone that settles onto its pitch from `bend` above it."""
    first = int(start * RATE)
    length = min(len(buffer) - first, int(decay * 9.2 * RATE))
    phase = 0.0
    for index in range(length):
        time = index / RATE
        phase += 2 * math.pi * frequency * (1 + bend * math.exp(-time / 0.02)) / RATE
        envelope = level * rise(time, attack) * math.exp(-time / decay)
        buffer[first + index] += envelope * (math.sin(phase) + 0.16 * math.sin(2 * phase))


# Comb and all-pass delays of the room, in samples, for the left and the right ear. The two sets
# differ by unequal amounts so that the tails are unlike and not opposite.
EARS = [
    ([1116, 1188, 1277, 1356, 1422, 1491, 1557, 1617], [556, 441, 341, 225]),
    ([1129, 1201, 1259, 1373, 1433, 1481, 1567, 1637], [569, 433, 347, 233]),
]


def diffuse(buffer, combs, passes):
    """Return the tail of a small, dark room for one ear, without the dry sound."""
    tail = [0.0] * len(buffer)
    # Eight damped combs give a smooth tail; fewer combs ring like metal.
    for delay in combs:
        line = [0.0] * delay
        damped = 0.0
        for index, sample in enumerate(buffer):
            out = line[index % delay]
            damped = out * 0.55 + damped * 0.45
            line[index % delay] = sample + damped * 0.6
            tail[index] += out * 0.125
    for delay in passes:
        line = [0.0] * delay
        for index, sample in enumerate(tail):
            out = line[index % delay] - sample * 0.5
            line[index % delay] = sample + out * 0.5
            tail[index] = out
    return tail


def room(buffer, wet):
    """Return the left and right channels: the dry sound in the middle of a short stereo tail."""
    return [[dry + wet * tail for dry, tail in zip(buffer, diffuse(buffer, combs, passes))] for combs, passes in EARS]


def chime():
    """Two soft electric-piano notes that rise a fifth: the default, warm and clear."""
    buffer = [0.0] * int(0.85 * RATE)
    tine(buffer, 0.0, 587.33, 0.11, 1.2, 0.8)
    tine(buffer, 0.125, 880.0, 0.13, 1.0, 1.0)
    return room(buffer, 0.35)


def bell():
    """One small bell: a low hum under a clear strike tone with a slow shimmer."""
    buffer = [0.0] * int(0.95 * RATE)
    partials = [
        (0.5, 0.16, 0.2), (1.0, 1.0, 0.15), (1.003, 0.3, 0.15), (2.0, 0.34, 0.09),
        (3.0, 0.12, 0.055), (4.2, 0.05, 0.035), (5.4, 0.025, 0.02),
    ]
    strike(buffer, 0.0, 739.99, partials, 1.0, attack=0.004)
    return room(buffer, 0.35)


def glass():
    """Two light, high notes a fifth apart, struck almost together: short and airy."""
    buffer = [0.0] * int(0.6 * RATE)
    rim = [(1, 1.0, 0.085), (2.32, 0.08, 0.035), (4.25, 0.02, 0.015)]
    strike(buffer, 0.0, 987.77, rim, 0.9, attack=0.005)
    strike(buffer, 0.045, 1479.98, rim, 0.75, attack=0.005)
    return room(buffer, 0.45)


def pulse_pair():
    """Two rounded pops on one note, the second softer: the shortest and the plainest."""
    buffer = [0.0] * int(0.5 * RATE)
    pop(buffer, 0.0, 659.26, 0.04, 0.07, 1.0)
    pop(buffer, 0.13, 659.26, 0.05, 0.07, 0.7)
    return room(buffer, 0.25)


def ascend():
    """A quick rising major arpeggio on a wooden bar that lands on the octave: a clear "done"."""
    buffer = [0.0] * int(0.9 * RATE)
    bar = [(1, 1.0, 0.07), (2, 0.2, 0.035), (4, 0.11, 0.018)]
    ring = [(1, 1.0, 0.12), (2, 0.2, 0.06), (4, 0.11, 0.03)]
    for index, frequency in enumerate([440.0, 554.37, 659.26]):
        strike(buffer, index * 0.075, frequency, bar, 0.62 + index * 0.1)
    strike(buffer, 0.225, 880.0, ring, 1.0)
    return room(buffer, 0.35)


DESIGNS = {"chime": chime, "bell": bell, "glass": glass, "pulse": pulse_pair, "ascend": ascend}


def master(channels):
    """Normalize to the shared peak, fade the end to silence and return interleaved 16-bit samples."""
    fade = int(0.12 * RATE)
    for channel in channels:
        for index in range(fade):
            channel[-1 - index] *= 0.5 - 0.5 * math.cos(math.pi * index / fade)
    scale = PEAK / max(abs(sample) for channel in channels for sample in channel)
    return [round(sample * scale * 32767) for frame in zip(*channels) for sample in frame]


def levels(path):
    """Describe a sound: length, peak, the level of its loudest 200 ms and when it becomes quiet."""
    with wave.open(str(path)) as audio:
        frames = audio.getnframes()
        channels = audio.getnchannels()
        samples = struct.unpack(f"<{frames * channels}h", audio.readframes(frames))
        rate = audio.getframerate()
    # The power of a frame is the mean over its channels.
    power = [sum(sample * sample for sample in samples[index:index + channels]) / channels for index in range(0, len(samples), channels)]
    window = int(rate * 0.2)
    energy = loudest = sum(power[:window])
    for index in range(window, frames):
        energy += power[index] - power[index - window]
        loudest = max(loudest, energy)
    # A sound is quiet after its last 10 ms that is within 40 dB of its loudest 10 ms.
    step = rate // 100
    steps = [sum(power[index:index + step]) for index in range(0, frames, step)]
    quiet = (max(index for index, value in enumerate(steps) if value > max(steps) * 1e-4) + 1) / 100
    peak = 20 * math.log10(max(abs(sample) for sample in samples) / 32768)
    body = 20 * math.log10(math.sqrt(loudest / window) / 32768)
    return f"{path.name}: {frames / rate:.2f} s, quiet after {quiet:.2f} s, peak {peak:.1f} dBFS, loudest 200 ms {body:.1f} dBFS, ends at {samples[-1]}"


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
                audio.setnchannels(2)
                audio.setsampwidth(2)
                audio.setframerate(RATE)
                audio.writeframes(struct.pack(f"<{len(samples)}h", *samples))
        print(levels(path))


if __name__ == "__main__":
    main()
