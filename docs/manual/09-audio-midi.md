# Audio, MIDI and recording

All settings are under *Options*; they apply and save at once.

## Audio output

*Options → Audio settings…*: the output **device** (or *System default*),
**buffer** size and **sample rate**. A smaller buffer means less delay
between a key and its sound, and more work for the computer. *Restart
audio* reopens the device.

KeyFrameXL plays through PipeWire (via its ALSA layer) or plain ALSA. If
the device disappears (a USB interface unplugged, PipeWire restarted)
the program reconnects by itself and says so in the status bar.

## MIDI input

*Options → MIDI settings…* (`F10`) lists MIDI inputs and outputs. For an
input you can set the channel (*Omni* takes all), *Transpose*, the
*Velocity* response, the *Sustain pedal* (CC 64 holds notes) and
*Quantize* (recorded notes snap to the grid).

**MIDI Clock**: *Master* sends 24 PPQN clock and Start/Stop to the enabled
outputs; *Slave* follows the clock of another device.

`Ctrl+T` plays the active channel from the computer keyboard. `Ctrl+H`
stops every sound (MIDI panic).

## Recording

`R` switches recording on; recording starts when you press Play.

- **Notes**: notes you play on a MIDI keyboard go into the current pattern
  of the active channel.
- **Automation**: knobs you move are written as automation.
- **Audio**: arm a mixer track (the dot on its strip) or the audio input
  (a microphone, line in). Up to 16 tracks record at once. Each pass
  while Record is on makes a take: a numbered WAV file in
  `~/Music/KeyFrameXL/Recordings` (*Browser → Files → Recordings*).
  `KEYFRAMEXL_RECORDINGS` sets another folder.

`Ctrl+M` switches the metronome on and off.
