# DWP validation: initial fixture plan

No DWP, related samples, MIDI reference or DirectWave render was present in the project during inventory. No DWP compatibility is claimed yet. Do not redistribute private material.

1. Obtain a user-selected bass, keys or pad DWP with its external sample folder, if applicable. Preserve the original bytes and note version, source and any missing files.
2. Prepare a MIDI phrase with notes just inside and outside zone and velocity boundaries, a repeated attack, chord, long sustain and release. Record its tempo and sample rate. If DirectWave is available, render dry and original-settings references at matching settings.
3. Inspect the preset through ConvertWithMoss 20.2.0 or later and SFZ. Record the exact version, license, artifact size, runtime and command before considering distribution. Conversion output is an observation, not proof of fidelity.
4. Compare root/ranges, gain, pan, tuning, velocity, start/end and loop mode/positions, ADSR, filter and LFO. Mark every parameter reproduced, approximated, omitted or unverified, preserving unknown blocks as unverified.
5. Test separately FLAC monolith, legacy PCM monolith and external WAV; stereo and attack; a deliberately missing file and Relink; relocation and packed project. Use an impulse at known frame positions for loop offsets. Missing fixture variants remain unverified.
6. Keep reader, resource resolution and voice engine separate. Persist normalized instrument, samples, original and report so playback does not need a converter.

Reference: https://github.com/git-moss/ConvertWithMoss/releases/tag/20.2.0

## Synthetic test, 2026-11-14

The generated fixture in `tests/fixtures/dwp` is a 48 kHz mono, 16-bit C4 sine wave, 24,000 frames; its SFZ maps keys 48–72 with root 60. ConvertWithMoss 20.3.0 (upstream LGPL-3.0, 52,701,888-byte Ubuntu package, bundled Java runtime; not redistributed) converted SFZ → monolithic DWP → SFZ/WAV. The returned root and limits were 60/48/72; the recovered WAV starts with the same source PCM bytes and has 24,064 frames, so its 64-frame tail difference is recorded rather than treated as an exact match. No DirectWave reference was available. The KeyFrameXL importer was exercised on the resulting DWP using `KEYFRAMEXL_CONVERTER` and verified the decoded audio and mapping. At that initial stage the importer rejected multi-zone programs and reported mono downmix and missing velocity, loop, envelope, filter, modulation and FX support. The implementation below supersedes those limits. No claim is made for those fields or other DWP variants.


## Multi-zone test, 2026-11-14

The source in `tests/fixtures/multizone/source/Zones.sfz` contains four original stereo WAVs, each 4,800 frames at 48 kHz. The quiet layer is a sine; the loud layer adds a third harmonic. The lower and upper key ranges use different sample roots. Source tones are periodic 125/250 Hz control signals with roots assigned to 48/60 so frame-aligned loops can be checked exactly. They are deliberately reference signals rather than tuned concert presets.

| Zone | Keys | Velocity | Root | Attack / decay / sustain / release |
|---|---|---|---|---|
| Low soft | 48–59 | 1–63 | 48 | .08 s / .12 s / 60% / .22 s |
| Low bright | 48–59 | 64–127 | 48 | .005 s / .07 s / 80% / .40 s |
| High soft | 60–72 | 1–63 | 60 | .20 s / .15 s / 40% / .25 s |
| High bright | 60–72 | 64–127 | 60 | .002 s / .15 s / 70% / .60 s |

All four loops cover source frames [384, 4224), converted from SFZ's inclusive loop end 4223. The ConvertWithMoss 20.3.0 roundtrip preserves these values and the first 4,800 stereo PCM frames exactly; each recovered WAV is padded to 5,120 frames. No padding is included inside the loop. The importer expands inherited global and group opcodes into each region. ADSR values agree within 1 μs / 0.000001 sustain fraction. The sampler uses linear ADSR curves, an approximation of DirectWave's curves.

Automated acceptance checks cover both sides of key 59/60 and velocity 63/64, rejection outside the ranges and zero velocity, preserved stereo PCM, continued sound after the original sample ends, quiet attack versus an instantaneous control, and release energy followed by silence. Forward, sustain and ping-pong boundary behaviour also has an independent frame-level control test. The DWP fixture specifically exercises continuous forward loops.

For the portability check, the input DWP is deleted after importing; the project is saved, copied to another directory, and the initial project file is removed. Reopening the relocated project restores all embedded sample data and parameters without invoking the converter. Its WAV matches the original render byte for byte. `artifacts/dwp-multizone` contains the reopened project, both renders and the import report. Four two-second bars play keys 48,48,60,60 at velocities 40,110,40,110, followed by the release tail.

Reproduce (set the converter path to the local installation):

```sh
python3 scripts/make_dwp_fixture.py --converter "$KEYFRAMEXL_CONVERTER"
KEYFRAMEXL_ARTIFACTS=artifacts/dwp-multizone cargo test multizone_ranges_loops_adsr_and_disk_roundtrip
```

Unverified: comparison to DirectWave, legacy PCM monolith and external-WAV fixtures in this pass, filter/LFO/FX and exact envelope curvature. Imported normalized SFZ parameters that the engine does not implement are listed in the report; unknown original DWP blocks remain unverified. One-shot gates and sustain ping-pong loops are explicitly rejected rather than silently changed. Earlier single-zone project documents are migrated on load.
