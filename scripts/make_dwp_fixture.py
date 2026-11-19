#!/usr/bin/env python3
"""Create original, redistributable sampler controls and a DirectWave fixture."""
import argparse, math, pathlib, struct, subprocess, wave
p=argparse.ArgumentParser();p.add_argument('--converter', required=True);args=p.parse_args()
root=pathlib.Path(__file__).resolve().parents[1]/'tests/fixtures/multizone'
source=root/'source';source.mkdir(parents=True,exist_ok=True)
regions=[]
for name,key,low,high,lo_vel,hi_vel,freq,bright,attack,decay,sustain,release in [
 ('Low soft',48,48,59,1,63,125,False,.08,.12,60,.22),
 ('Low bright',48,48,59,64,127,125,True,.005,.07,80,.40),
 ('High soft',60,60,72,1,63,250,False,.20,.15,40,.25),
 ('High bright',60,60,72,64,127,250,True,.002,.15,70,.60),
]:
 path=source/(name+'.wav')
 # Exact periodic waveform makes the loop endpoints measurable in frames.
 with wave.open(str(path),'wb') as w:
  w.setnchannels(2);w.setsampwidth(2);w.setframerate(48000)
  data=[]
  for i in range(4800):
   phase=2*math.pi*freq*i/48000
   value=(math.sin(phase)+(.3*math.sin(3*phase) if bright else 0))*.35
   data.append(struct.pack('<hh',round(value*32767),round(value*.65*32767)))
  w.writeframes(b''.join(data))
 regions.append(f'<region>\nsample={path.name}\nlokey={low} hikey={high} pitch_keycenter={key} lovel={lo_vel} hivel={hi_vel}\nloop_mode=loop_continuous loop_start=384 loop_end=4223\nampeg_attack={attack} ampeg_decay={decay} ampeg_sustain={sustain} ampeg_release={release}\namp_veltrack=100\n')
(source/'Zones.sfz').write_text('\n'.join(regions))
out=root/'converted';out.mkdir(exist_ok=True)
result=subprocess.run([args.converter,'-s','sfz','-d','directwave',str(source/'Zones.sfz'),str(out)],capture_output=True,text=True,check=True)
(root/'creation.log').write_text(result.stdout+result.stderr)
print(result.stdout)
