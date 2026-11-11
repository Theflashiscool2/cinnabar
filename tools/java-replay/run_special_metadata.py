"""Head/portal publication remains stable during batched nonvisual skull-data updates."""
import argparse,json,sys,threading
from pathlib import Path
from check import rpc,private_json
from run_fixture import until
from harness import rcon
from special_fixture import options,config
p=options(__doc__);p.add_argument('--keep-client',action='store_true');a=p.parse_args();o=a.out;o.mkdir(mode=0o700,parents=True,exist_ok=False)
e=a.endpoint;c=config(a.fixture);cmd=lambda v:rpc(e,v);state=lambda:cmd({'cmd':'state'});server=lambda s:rcon(c,s)
def metrics(s):
 f=s['native_java_special_frame'];return {'frame':s['native_java_telemetry']['timings']['frame_interval']['samples'],'portal':f['portal_vertices'],'solid':f['solid_vertices'],'translucent':f['translucent_vertices'],'cache':s['native_java_special']['cached_columns'],'heads':s['native_java_special']['player_heads'],'pending':s['native_java_special']['artwork_pending']}
cmd({'cmd':'connect_java_fixture','address':f"127.0.0.1:{c['port']}"});until(state,lambda s:s['native_java_render']['loading_stage']=='ready','ready',timeout=90)
receipts=[]
for x in range(5,8):
 for z in range(5,8):
  receipts.append(server(f'setblock {x*16+10} 70 {z*16+10} minecraft:end_portal'))
  receipts.append(server(f'setblock {x*16+12} 70 {z*16+10} minecraft:player_head'))
for pos,level in [('104 70 110',15),('104 71 110',0)]:receipts.append(server(f'setblock {pos} minecraft:light[level={level}]'))
server('kill @e[tag=cinnabar_special_probe]')
for x in range(5,8):
 for z in range(5,8):server(f'summon minecraft:marker {x*16+12} 70 {z*16+10} {{Tags:["cinnabar_special_probe"]}}')
server('gamemode spectator CinnabarTest');server('tp CinnabarTest 112.5 72 103.5 60 25')
until(state,lambda s:s['native_java_hud']['mode']=='Spectator' and abs(s['native_java']['motion']['position'][1]-72)<0.1,'spectator camera and correction')
s=until(state,lambda s:s['native_java_special']['player_heads']==9 and s['native_java_special']['cached_columns']==s['native_java']['columns'] and s['native_java']['columns']>=49 and s['native_java_special_frame']['portal_vertices']>0 and not s['native_java_special']['artwork_pending'] and s.get('render_pipeline_warmup_ready') is True,'all special columns and resources',timeout=90)
# Require settled publication across several actual frames before taking the reference counts.
start=metrics(s)['frame'];s=until(state,lambda s:metrics(s)['frame']>=start+8 and s['native_java_special']['player_heads']==9 and not s['native_java_special']['artwork_pending'],'settled')
initial=metrics(s);private_json(o/'initial.json',s);cmd({'cmd':'screenshot','path':str((o/'initial.png').resolve())});samples=[];first_drop=None
stop=threading.Event(); lock=threading.Lock(); trial=-1; dropped_state=None; errors=[]
def sampling():
 global first_drop,dropped_state
 try:
  while not stop.is_set():
   snapshot=state();m=metrics(snapshot);m['trial']=trial
   with lock:
    samples.append(m)
    if first_drop is None and (m['portal']<initial['portal'] or m['solid']<initial['solid'] or m['translucent']<initial['translucent']):
     first_drop=m;dropped_state=snapshot
 except Exception as error:errors.append(str(error));stop.set()
sampler=threading.Thread(target=sampling,daemon=True);sampler.start()
try:
 for trial in range(12):
  with lock:start=samples[-1]['frame'] if samples else initial['frame']
  sound='minecraft:block.note_block.hat' if trial%2==0 else 'minecraft:block.note_block.pling'
  server('execute as @e[tag=cinnabar_special_probe] at @s run data merge block ~ ~ ~ {note_block_sound:"'+sound+'"}')
  until(lambda:samples[-1] if samples else initial,lambda m:m['frame']>=start+8 or bool(errors),'concurrent light propagation frames',timeout=30)
finally:
 stop.set();sampler.join(timeout=15)
assert not errors and not sampler.is_alive(),errors
if dropped_state is not None:
 private_json(o/'first-drop.json',dropped_state)
 cmd({'cmd':'screenshot','path':str((o/'first-drop.png').resolve())})
summary={'initial':initial,'min_portal':min(m['portal'] for m in samples),'min_solid':min(m['solid'] for m in samples),'min_translucent':min(m['translucent'] for m in samples),'samples':samples,'first_drop':first_drop,'passed':first_drop is None,'scope':'Linux controlled vanilla; state-publication samples and screenshots, no Hypixel/iPhone claim'}
private_json(o/'result.json',summary);print({k:v for k,v in summary.items() if k!='samples'})
cmd({'cmd':'screenshot','path':str((o/'final.png').resolve())});server('kill @e[tag=cinnabar_special_probe]')
if not a.keep_client:cmd({'cmd':'disconnect'});cmd({'cmd':'quit'})
assert summary['passed'],'head/portal publication dropped during nonvisual block-entity updates'
