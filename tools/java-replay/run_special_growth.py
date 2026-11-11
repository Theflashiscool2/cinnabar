"""Existing head/portal publication survives ordinary terrain atlas extension."""
import argparse,json,sys,threading
from pathlib import Path
from check import rpc,private_json
from run_fixture import until
from harness import rcon
from special_fixture import options,config
p=options(__doc__);p.add_argument('--special',action='store_true');a=p.parse_args();o=a.out;o.mkdir(mode=0o700,parents=True,exist_ok=False)
e=a.endpoint;c=config(a.fixture);cmd=lambda v:rpc(e,v);state=lambda:cmd({'cmd':'state'});server=lambda v:rcon(c,v)
def metrics(s):
 f=s['native_java_special_frame'];return {'frame':s['native_java_telemetry']['timings']['frame_interval']['samples'],'portal':f['portal_vertices'],'solid':f['solid_vertices'],'translucent':f['translucent_vertices'],'cache':s['native_java_special']['cached_columns'],'heads':s['native_java_special']['player_heads'],'artwork_pending':s['native_java_special']['artwork_pending'],'has_atlas':f['has_atlas'],'extension':s['native_java_render']['graphics_extensions'],'pending':s['native_java_render']['graphics_extension_pending']}
s=until(state,lambda s:s.get('render_pipeline_warmup_ready') is True and not s['native_java_render']['graphics_extension_pending'] and s['native_java_special']['player_heads']==9,'initial ready')
initial=metrics(s);private_json(o/'initial.json',s);samples=[];stop=threading.Event();errors=[];dropped=[None]
def sample():
 try:
  while not stop.is_set():
   s=state();m=metrics(s);samples.append(m)
   if dropped[0] is None and any(m[k]<initial[k] for k in ['portal','solid','translucent']):dropped[0]=s
 except Exception as error:errors.append(str(error));stop.set()
t=threading.Thread(target=sample,daemon=True);t.start();receipts=[]
try:
 for block in (['conduit'] if a.special else ['diamond_block','emerald_block','redstone_block','prismarine']):
  before=metrics(state())['extension'];receipts.append(server('setblock 115 69 104 minecraft:'+block))
  until(lambda:samples[-1] if samples else initial,lambda m:m['extension']>before and not m['pending'] or bool(errors),'real graphics extension',timeout=90)
finally:
 stop.set();t.join(timeout=15);server('setblock 115 69 104 minecraft:air')
assert not errors and not t.is_alive(),errors
result={'initial':initial,'extensions':samples[-1]['extension']-initial['extension'],'minimum':{k:min(m[k] for m in samples) for k in ['portal','solid','translucent','cache','heads']},'passed':dropped[0] is None,'samples':samples,'server_receipts':receipts}
private_json(o/'result.json',result)
if dropped[0] is not None:private_json(o/'first-drop.json',dropped[0])
cmd({'cmd':'screenshot','path':str((o/'final.png').resolve())})
print({k:v for k,v in result.items() if k not in ['samples','server_receipts']})
assert result['passed'],'unchanged special blocks vanished during ordinary graphics extension'
