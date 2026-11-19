import channels
import math
import mixer

held = [set() for _ in SLOTS]
base_pitch = [None for _ in SLOTS]
pitch_range = [None for _ in SLOTS]
pitch_lsb = [0 for _ in SLOTS]


def set_pitch(slot, offset):
    index = SLOTS[slot]
    if base_pitch[slot] is None:
        base_pitch[slot] = channels.getChannelPitch(index, 1, True)
        pitch_range[slot] = channels.getChannelPitch(index, 2, True)
    pitch = base_pitch[slot] + offset
    if abs(pitch) > pitch_range[slot]:
        pitch_range[slot] = math.ceil(abs(pitch))
        channels.setChannelPitch(index, pitch_range[slot], 2, 0, True)
    channels.setChannelPitch(index, pitch / pitch_range[slot], 0, 0, True)


def OnMidiMsg(event):
    kind = event.status & 0xF0
    slot = event.status & 0x0F
    if kind == 0xB0 and slot < len(SLOTS) and event.data1 == 127:
        for note in held[slot]:
            channels.midiNoteOn(SLOTS[slot], note, 0)
        held[slot].clear()
        if base_pitch[slot] is not None:
            set_pitch(slot, 0)
        event.handled = True
    elif kind == 0xB0 and slot < len(SLOTS) and event.data1 == 118:
        pitch_lsb[slot] = event.data2
        event.handled = True
    elif kind == 0xB0 and slot < len(SLOTS) and event.data1 == 119:
        set_pitch(slot, (pitch_lsb[slot] + 128 * event.data2 - 8192) / 100)
        event.handled = True
    elif kind == 0xB0 and slot == 0:
        if hasattr(mixer, "setCurrentTempo"):
            mixer.setCurrentTempo((event.data1 * 128 + event.data2) / 10)
        event.handled = True
    elif kind in (0x80, 0x90) and slot < len(SLOTS):
        index = SLOTS[slot]
        velocity = event.data2 if kind == 0x90 else 0
        channels.midiNoteOn(index, event.data1, velocity)
        if velocity:
            held[slot].add(event.data1)
        else:
            held[slot].discard(event.data1)
        event.handled = True
