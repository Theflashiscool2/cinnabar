"""Authoritative removal, unload/revisit and reconnect of native special-block presentation."""
import sys,json,argparse
from pathlib import Path
from check import rpc,private_json
from run_fixture import until
from harness import rcon
from special_fixture import options,config
p=options(__doc__);a=p.parse_args();o=a.out;o.mkdir(mode=0o700,parents=True,exist_ok=False)
e=a.endpoint;c=config(a.fixture);cmd=lambda v:rpc(e,v);state=lambda:cmd({'cmd':'state'});server=lambda v:rcon(c,v)
s=state();initial=s['native_java_special_frame'];session=s['native_java_special']['session'];assert s['native_java_special']['player_heads']==9
portal=initial['portal_vertices'];trans=initial['translucent_vertices'];assert portal%9==0 and trans%9==0
expected_portal=portal-portal//9;expected_trans=trans-trans//9
ready=lambda s,heads,pv,tv:s['native_java_render']['loading_stage']=='ready' and s['native_java_special']['player_heads']==heads and s['native_java_special_frame']['portal_vertices']==pv and s['native_java_special_frame']['translucent_vertices']==tv
receipts=[server('setblock 106 70 106 minecraft:air'),server('setblock 108 70 106 minecraft:air')]
s=until(state,lambda s:ready(s,8,expected_portal,expected_trans),'removed special geometry')
private_json(o/'removed.json',s);cmd({'cmd':'screenshot','path':str((o/'removed.png').resolve())})
receipts += [server('execute if block 106 70 106 minecraft:air'),server('execute if block 108 70 106 minecraft:air')]
assert all('Test passed' in t for t in receipts[-2:])
server('tp CinnabarTest 1024.5 72 1024.5 0 0')
s=until(state,lambda s:ready(s,0,0,0) and s['native_java']['columns']==49,'unload old specials',timeout=90);private_json(o/'unloaded.json',s)
server('tp CinnabarTest 112.5 72 103.5 60 25')
s=until(state,lambda s:ready(s,8,expected_portal,expected_trans),'revisit without deleted special blocks',timeout=90);private_json(o/'revisited.json',s)
cmd({'cmd':'disconnect'})
until(state,lambda s:s['native_java'] is None and s['native_java_special_frame']['portal_vertices']==0 and s['native_java_special_frame']['translucent_vertices']==0,'disconnect cleanup')
cmd({'cmd':'connect_java_fixture','address':f"127.0.0.1:{c['port']}"})
s=until(state,lambda s:ready(s,8,expected_portal,expected_trans),'reconnect authoritative special state',timeout=90);assert s['native_java_special']['session']!=session;private_json(o/'reconnected.json',s)
cmd({'cmd':'screenshot','path':str((o/'reconnected.png').resolve())})
receipts += [server('setblock 106 70 106 minecraft:end_portal'),server('setblock 108 70 106 minecraft:player_head')]
s=until(state,lambda s:ready(s,9,portal,trans),'new special additions after reconnect');private_json(o/'restored.json',s)
private_json(o/'result.json',{'passed':True,'removed_then_recreated':True,'unload_revisit':True,'session_replaced':True,'server_receipts':receipts,'scope':'controlled graphical special renderer lifecycle; not portal traversal, Hypixel or iPhone'})
cmd({'cmd':'disconnect'});cmd({'cmd':'quit'});print('PASS: removal/addition, unload/revisit and reconnect with new session identity; no deleted geometry resurrected.')
