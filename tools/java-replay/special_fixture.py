"""Safety/arguments for destructive, isolated special-renderer fixtures."""
import argparse,json
from pathlib import Path
from wire import loopback

def options(description):
    p=argparse.ArgumentParser(description=description)
    p.add_argument('--endpoint',type=Path,required=True)
    p.add_argument('--fixture',type=Path,required=True)
    p.add_argument('--out',type=Path,required=True)
    return p

def config(path):
    c=json.loads(path.read_text())
    loopback(f"127.0.0.1:{c['port']}")
    properties=dict(line.split('=',1) for line in (path.parent/'server.properties').read_text().splitlines()
                    if '=' in line and not line.startswith('#'))
    if properties.get('online-mode')!='false' or properties.get('server-ip')!='127.0.0.1':
        raise ValueError('requires the isolated offline loopback harness')
    return c
