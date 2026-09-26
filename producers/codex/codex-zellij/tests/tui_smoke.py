#!/usr/bin/env python3
"""Installed Codex TUI -> actual launcher -> isolated installed server, no inference."""
import fcntl
import json
import os
from pathlib import Path
import pty
import select
import signal
import struct
import subprocess
import tempfile
import termios
import time

BINARY=Path(__file__).resolve().parents[1]/'target/debug/codex-zellij'


def smoke():
    with tempfile.TemporaryDirectory(prefix='zca-tui-',dir='/tmp') as temporary:
        root=Path(temporary)
        home=root/'home'; home.mkdir()
        (home/'config.toml').write_text('model_provider="probe"\nmodel="probe"\n'
            '[model_providers.probe]\nname="probe"\nwire_api="responses"\nbase_url="http://127.0.0.1:1/v1"\n'
            f'[projects.{json.dumps(str(root))}]\ntrust_level="trusted"\n')
        env={k:v for k,v in os.environ.items() if not any(x in k for x in ['TOKEN','API_KEY','CODEX','OPENAI','ZELLIJ'])}
        env.update(CODEX_HOME=str(home),TERM='xterm-256color',ZELLIJ_SESSION_NAME=root.name,
                   ZELLIJ_PANE_ID='46',ZELLIJ_AGENT_ACTIVITY_COORDINATION_DIR=str(root/'coordination'))
        master,slave=pty.openpty()
        fcntl.ioctl(slave,termios.TIOCSWINSZ,struct.pack('HHHH',40,120,0,0))
        with (root/'server.log').open('w') as log:
            from probe_installed import Rpc
            package=Path(__file__).resolve().parents[4]/'target/codex-proxy-marketplace'
            for args in [('marketplace','add',str(package)),('add','zellij-agent-activity@zellij-agent-activity')]:
                subprocess.run(['codex','plugin',*args],env=env,cwd=root,check=True,capture_output=True,timeout=30)
            control=Rpc(env,root,log)
            try:
                hooks=control.call('hooks/list',{'cwds':[str(root)]})['data'][0]['hooks']
                assert len(hooks)==7, 'staged producer inventory incomplete'
            finally: control.close()
            with (home/'config.toml').open('a') as config:
                for hook in hooks:
                    config.write(f'\n[hooks.state.{json.dumps(hook["key"])}]\ntrusted_hash={json.dumps(hook["currentHash"])}\n')
            server=subprocess.Popen(['codex','app-server','--listen',f'unix://{root}/server.sock'],env=env,cwd=root,
                                    stdin=subprocess.DEVNULL,stdout=log,stderr=log)
            client=None
            try:
                deadline=time.monotonic()+10
                while not (root/'server.sock').exists() and time.monotonic()<deadline: time.sleep(.02)
                assert (root/'server.sock').exists(), 'isolated server did not start'
                client=subprocess.Popen([str(BINARY),'--daemon',f'unix://{root}/server.sock','--','--no-alt-screen'],
                                        env=env,cwd=root,stdin=slave,stdout=slave,stderr=slave)
                os.close(slave); slave=None
                screen=b''
                deadline=time.monotonic()+20
                while time.monotonic()<deadline:
                    if select.select([master],[],[],.1)[0]:
                        try: data=os.read(master,65536)
                        except OSError: break
                        screen+=data
                        if b'\x1b[6n' in data: os.write(master,b'\x1b[1;1R')
                        if b'\x1b]11;?' in data: os.write(master,b'\x1b]11;rgb:0000/0000/0000\x1b\\')
                    # A generated per-thread ownership file proves the real client selected a
                    # conversation through the proxy, beyond merely displaying a startup screen.
                    if list((root/'coordination').glob('*.owners')):
                        break
                    if client.poll() is not None: break
                assert list((root/'coordination').glob('*.owners')), 'TUI did not select a conversation: '+screen.decode(errors='replace')[-2000:]
                client.send_signal(signal.SIGTERM)
                assert client.wait(timeout=8)==0
                print('PASS: installed Codex TUI selected a conversation through the normal launcher; clean SIGTERM shutdown.')
            finally:
                if client is not None and client.poll() is None: client.kill(); client.wait(timeout=5)
                server.terminate()
                try: server.wait(timeout=5)
                except subprocess.TimeoutExpired: server.kill(); server.wait(timeout=5)
                os.close(master)
                if slave is not None: os.close(slave)


if __name__=='__main__': smoke()
