"""Actual client controls and native packet order; local offline fixture only."""
import argparse,json,socket,threading,time
from pathlib import Path
from check import rpc,private_json
from run_fixture import until
from harness import rcon
from session import capture
from tape import Writer,load
from wire import name,read_vi,loopback,FIXTURE_NAME

def check_contact_order(mode, events):
    """The pinned initial gesture survives denial and follows START when START is allowed."""
    assert any(e[0] == 'ServerboundPunchPacket' for e in events), events
    if mode == 'adventure':
        assert not any(e[0] == 'ServerboundPlayerActionPacket' for e in events), events
    elif mode == 'survival':
        start = next(i for i, e in enumerate(events) if e == ['ServerboundPlayerActionPacket', 0])
        punch = next(i for i, e in enumerate(events) if e == ['ServerboundPunchPacket'])
        assert start < punch, events
    else:
        raise ValueError('unsupported contact fixture mode')

def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--out',type=Path,required=True);p.add_argument('--endpoint',type=Path,required=True);p.add_argument('--fixture',type=Path,required=True);p.add_argument('--port',type=int,default=25589);a=p.parse_args()
    o=a.out;o.mkdir(mode=0o700,parents=True,exist_ok=False)
    e=a.endpoint;config=json.loads(a.fixture.read_text());loopback(f"127.0.0.1:{config['port']}")
    properties = dict(line.split('=', 1) for line in (a.fixture.parent / 'server.properties').read_text().splitlines()
                      if '=' in line and not line.startswith('#'))
    if properties.get('online-mode') != 'false' or properties.get('server-ip') != '127.0.0.1':
        raise ValueError('requires the isolated offline loopback harness')
    cmd=lambda c:rpc(e,c);state=lambda:cmd({'cmd':'state'});server=lambda t:rcon(config,t)
    assert state()['native_java'] is None
    listener=socket.socket();listener.setsockopt(socket.SOL_SOCKET,socket.SO_REUSEADDR,1);listener.bind(('127.0.0.1',a.port));listener.listen(1)
    w=Writer(o/'session.jsonl');errors=[]
    def proxy():
     try:
      client,_=listener.accept()
      with client:capture(client,('127.0.0.1',config['port']),w,0,a.port)
     except Exception as error:errors.append(str(error))
    t=threading.Thread(target=proxy,daemon=True);t.start();windows={};result={}
    try:
     cmd({'cmd':'connect_java_fixture','address':f'127.0.0.1:{a.port}'})
     until(state,lambda s:s['native_java_render']['loading_stage']=='ready','ready',timeout=90)
     server(f'clear {FIXTURE_NAME}');server(f'tp {FIXTURE_NAME} 104.5 70 104.5 180 90')
     for mode in ['adventure','survival']:
      server(f'gamemode {mode} {FIXTURE_NAME}')
      until(state,lambda s:s['native_java_hud']['mode'].lower()==mode and not s['native_java_movement']['pending_frame'],'mode')
      cmd({'cmd':'input','hotbar':6,'look':{'yaw':180,'pitch':90}})
      before=state();private_json(o/(mode+'-before.json'),before)
      n=before['native_java_telemetry']['counts']['local_swings_started']['count'];start=time.monotonic_ns()-w.started
      cmd({'cmd':'input','hold':['MouseLeft']})
      after=until(state,lambda s:s['native_java_telemetry']['counts']['local_swings_started']['count']>n,'initial '+mode+' contact swing',timeout=5)
      private_json(o/(mode+'-swing.json'),after)
      cmd({'cmd':'screenshot','path':str((o/(mode+'-swing.png')).resolve())})
      cmd({'cmd':'input','release_all':True})
      tick=after['native_java_telemetry']['timings']['frame_interval']['samples']
      until(state,lambda s:s['native_java_telemetry']['timings']['frame_interval']['samples']>=tick+3,'settled frames')
      windows[mode]=[start,time.monotonic_ns()-w.started]
      result[mode]={'new_swings':after['native_java_telemetry']['counts']['local_swings_started']['count']-n,'block':after['native_java_interaction']['selected'],'observed_mode':after['native_java_hud']['mode']}
     cmd({'cmd':'disconnect'});until(state,lambda s:s['native_java'] is None,'disconnect');t.join(timeout=15);assert not t.is_alive() and not errors,errors
     w.close();rows=load(o/'session.jsonl')[0]['packets']
     for mode,(start,end) in windows.items():
      packets=[r for r in rows if r['direction']=='client' and r['phase']=='play' and start<=r['ns']<=end]
      events=[]
      for r in packets:
       packet=name(r['body'],'play',False)
       if packet=='ServerboundPlayerActionPacket':
        _,offset=read_vi(r['body']);action,_=read_vi(r['body'],offset);events.append([packet,action])
       elif packet=='ServerboundPunchPacket':events.append([packet])
      check_contact_order(mode, events)
      result[mode]['native_wire_events']=events
     private_json(o/'result.json',{'passed':True,'cases':result,'capture_errors':errors,'scope':'Linux controlled vanilla; not Hypixel/iPhone'})
     print(json.dumps(result))
    except Exception as error:
     private_json(o/'failure.json', {'error':str(error)})
     cmd({'cmd':'screenshot','path':str((o/'failure.png').resolve())})
     raise
    finally:
     cmd({'cmd':'input','release_all':True})
     if state()['native_java'] is not None:cmd({'cmd':'disconnect'})
     t.join(timeout=15)
     if not w.file.closed:w.abort()
     if t.is_alive():raise ValueError('capture did not retire')
     listener.close()

if __name__ == "__main__":
    main()
