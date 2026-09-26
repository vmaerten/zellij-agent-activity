#!/usr/bin/env python3
"""Isolated installed-Codex probe; no credentials, live sessions, or model calls."""
import json
import os
from pathlib import Path
import queue
import subprocess
import tempfile
import time
import threading
import select
import sys


class Rpc:
    def __init__(self, env, cwd, log):
        self.process = subprocess.Popen(
            ['codex', 'app-server', '--stdio'],
            env=env, cwd=cwd, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
            stderr=log, text=True,
        )
        self.messages = queue.Queue()
        threading.Thread(target=self.read, daemon=True).start()
        self.next_id = 0
        self.events = []
        self.call('initialize', {'clientInfo': {'name': 'activity_probe', 'version': '0.1.0'},
                                 'capabilities': {'experimentalApi': True}})
        self.send({'method': 'initialized'})

    def read(self):
        for line in self.process.stdout:
            self.messages.put(json.loads(line))

    def send(self, message):
        self.process.stdin.write(json.dumps(message) + '\n')
        self.process.stdin.flush()

    def call(self, method, params):
        self.next_id += 1
        self.send({'id': self.next_id, 'method': method, 'params': params})
        while True:
            msg = self.messages.get(timeout=30)
            if msg.get('id') == self.next_id and 'method' not in msg:
                if 'error' in msg:
                    raise RuntimeError(msg['error'])
                return msg['result']
            self.events.append(msg)

    def close(self):
        self.process.terminate()
        try:
            self.process.wait(timeout=5)
        except subprocess.TimeoutExpired:
            self.process.kill()
            self.process.wait()


def run_turn(rpc, thread):
    result = rpc.call('turn/start', {'threadId': thread,
        'input': [{'type': 'text', 'text': 'probe', 'text_elements': []}]})
    finish_turn(rpc, thread, result['turn']['id'])


def finish_turn(rpc, thread, turn):
    interrupted = False
    index = 0
    while True:
        if index < len(rpc.events):
            msg = rpc.events[index]
            index += 1
        else:
            msg = rpc.messages.get(timeout=30)
            rpc.events.append(msg)
            index += 1
        params = msg.get('params', {})
        if msg.get('method') == 'error' and params.get('turnId') == turn and not interrupted:
            interrupted = True
            rpc.call('turn/interrupt', {'threadId': thread, 'turnId': turn})
        if msg.get('method') == 'turn/completed' and params['turn']['id'] == turn:
            return


def coordinated_probe(rpc, root, marker):
    from handoff import OWNER, until
    thread=rpc.call('thread/start', {'cwd':str(root)})['thread']['id']
    run_turn(rpc,thread)
    assert marker.exists(), 'control hook never emitted'
    other=rpc.call('thread/start',{'cwd':str(root)})['thread']['id']
    run_turn(rpc,other)
    marker.write_text('')
    (root/'hold').touch()
    turn=rpc.call('turn/start',{'threadId':thread,'input':[{'type':'text','text':'probe','text_elements':[]}]})['turn']['id']
    until(lambda:(root/'entered').exists(),'old hook never reached its Zellij subprocess')
    rpc.call('thread/resume', {'threadId':thread})
    owners=[]
    def acquire():
        owner=subprocess.Popen([str(OWNER),str(root/'coordination'),thread],stdin=subprocess.PIPE,
                               stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True)
        owners.append(owner)
        assert owner.stdout.readline().strip()=='acquiring'
        return owner
    try:
        first=acquire()
        assert not select.select([first.stdout],[],[],.1)[0], 'ownership acquired before old hook drained'
        (root/'release').touch()
        assert first.stdout.readline().strip()=='owned'
        finish_turn(rpc,thread,turn)
        marker.write_text('')
        run_turn(rpc,thread)
        assert marker.read_text()=='', 'managed Codex hook still emitted after ownership'
        run_turn(rpc,other)
        assert any(json.loads(line)['session_id']==other for line in marker.read_text().splitlines())
        second=acquire()
        assert second.stdout.readline().strip()=='owned'
        first.kill(); first.wait(timeout=5)
        marker.write_text('')
        run_turn(rpc,thread)
        assert marker.read_text()=='', 'one departing view released another view ownership'
        second.kill(); second.wait(timeout=5)
        run_turn(rpc,thread)
        assert any(json.loads(line)['session_id']==thread for line in marker.read_text().splitlines()), 'last view did not restore Codex hooks'
        print('PASS on installed Codex: hot resume drained; no competing hooks; unrelated thread preserved; last view restores hooks without config mutation.')
    finally:
        for owner in owners:
            if owner.poll() is None: owner.kill(); owner.wait(timeout=5)


def probe(coordinated=False):
    with tempfile.TemporaryDirectory(prefix='zca-probe-', dir='/tmp') as directory:
        root = Path(directory)
        home = root / 'home'
        home.mkdir()
        market = root / 'market'
        plugin = market / 'producer'
        (market / '.agents/plugins').mkdir(parents=True)
        (plugin / '.codex-plugin').mkdir(parents=True)
        (plugin / 'hooks').mkdir()
        marker = root / 'events.jsonl'
        (market / '.agents/plugins/marketplace.json').write_text(json.dumps({
            'name': 'activity-probe', 'plugins': [{'name': 'activity-probe', 'source': './producer'}]}))
        (plugin / '.codex-plugin/plugin.json').write_text(json.dumps({
            'name': 'activity-probe', 'version': '0.1.0', 'description': 'Isolated hook probe'}))
        # This is the only trusted hook. It records identifiers, never prompt/tool content.
        script = plugin / 'record.py'
        script.write_text('import json,sys,time,pathlib\np=json.load(sys.stdin)\n'
                          f'root=pathlib.Path({str(root)!r})\n'
                          'if p.get("hook_event_name")=="UserPromptSubmit" and (root/"hold").exists():\n'
                          ' (root/"entered").touch()\n'
                          ' deadline=time.monotonic()+10\n'
                          ' while not (root/"release").exists() and time.monotonic()<deadline: time.sleep(.01)\n'
                          f'with open({str(marker)!r}, "a") as f:\n'
                          ' f.write(json.dumps({k:p.get(k) for k in ["session_id","hook_event_name"]})+"\\n")\n')
        if coordinated:
            forwarder = Path(__file__).resolve().parents[2] / 'scripts/forwarder.sh'
            script.write_text('import json,sys,os,subprocess\np=json.load(sys.stdin)\n'
                'env=dict(os.environ,ZCA_THREAD=p["session_id"],ZCA_EVENT=p["hook_event_name"])\n'
                f'subprocess.run(["bash",{str(forwarder)!r}],input=json.dumps(p),text=True,env=env,check=True)\n')
            sink=root/'zellij'
            sink.write_text('#!/usr/bin/env python3\nimport json,os,pathlib,time\n'
                f'root=pathlib.Path({str(root)!r})\n'
                'if os.environ["ZCA_EVENT"]=="UserPromptSubmit" and (root/"hold").exists():\n'
                ' (root/"entered").touch()\n'
                ' while not (root/"release").exists(): time.sleep(.01)\n'
                'with (root/"events.jsonl").open("a") as f:\n'
                ' f.write(json.dumps({"session_id":os.environ["ZCA_THREAD"],"hook_event_name":os.environ["ZCA_EVENT"]})+"\\n")\n')
            sink.chmod(0o700)
        (plugin / 'hooks/hooks.json').write_text(json.dumps({'hooks': {
            event: [{'hooks': [{'type': 'command', 'command': f'python3 {script}', 'timeout': 12}]}]
            for event in ['SessionStart', 'UserPromptSubmit']}}))
        env = {k: v for k, v in os.environ.items() if not any(x in k for x in ['TOKEN', 'API_KEY', 'CODEX', 'OPENAI', 'ZELLIJ'])}
        env.update(CODEX_HOME=str(home))
        if coordinated:
            env.update(PATH=str(root)+':'+env['PATH'],ZELLIJ_SESSION_NAME='legacy',ZELLIJ_PANE_ID='26',
                       ZELLIJ_AGENT_ACTIVITY_COORDINATION_DIR=str(root/'coordination'))
        # The loopback provider is deliberately unavailable: UserPromptSubmit runs before inference.
        (home / 'config.toml').write_text('model_provider="probe"\nmodel="probe"\n'
            '[model_providers.probe]\nname="probe"\nbase_url="http://127.0.0.1:1/v1"\nwire_api="responses"\n'
            'request_max_retries=0\nstream_max_retries=0\n')
        for args in [('marketplace', 'add', str(market)), ('add', 'activity-probe@activity-probe')]:
            subprocess.run(['codex', 'plugin', *args], env=env, cwd=root,
                           check=True, capture_output=True, timeout=30)
        with (root / 'server.log').open('w') as log:
            rpc = Rpc(env, root, log)
            try:
                hooks = rpc.call('hooks/list', {'cwds':[str(root)]})['data'][0]['hooks']
                assert len(hooks) == 2
                rpc.close()
                with (home / 'config.toml').open('a') as config:
                    for hook in hooks:
                        config.write(f'\n[hooks.state.{json.dumps(hook["key"])}]\ntrusted_hash={json.dumps(hook["currentHash"])}\n')
                rpc = Rpc(env, root, log)
                if coordinated:
                    coordinated_probe(rpc, root, marker)
                    return
                key = 'plugins.activity-probe@activity-probe.enabled'
                plain = rpc.call('thread/start', {'cwd': str(root)})['thread']['id']
                run_turn(rpc, plain)
                assert marker.exists(), 'control hook did not run'
                def events(thread):
                    return [x['hook_event_name'] for x in map(json.loads, marker.read_text().splitlines()) if x['session_id'] == thread]
                assert 'UserPromptSubmit' in events(plain)
                marker.write_text('')
                disabled = rpc.call('thread/start', {'cwd': str(root), 'config': {key: False}})['thread']['id']
                run_turn(rpc, disabled)
                assert not events(disabled), 'new-thread suppression failed'
                rpc.call('thread/resume', {'threadId': plain, 'config': {key: False}})
                run_turn(rpc, plain)
                resume_events = events(plain)
                assert 'UserPromptSubmit' in resume_events
                marker.write_text('')
                rpc.call('thread/settings/update', {'threadId': plain, 'disabledPluginIds': ['activity-probe@activity-probe']})
                run_turn(rpc, plain)
                assert not events(plain), 'settings suppression no longer works'
                ordinary = rpc.call('thread/start', {'cwd': str(root)})['thread']['id']
                run_turn(rpc, ordinary)
                assert 'UserPromptSubmit' in events(ordinary), 'settings leaked to another thread'
                # A hook already executing at attachment cannot be revoked by the settings RPC.
                rpc.call('thread/settings/update', {'threadId': plain, 'disabledPluginIds': []})
                (root / 'hold').touch()
                turn = rpc.call('turn/start', {'threadId': plain, 'input': [{'type':'text','text':'probe','text_elements':[]}]})['turn']['id']
                deadline = time.monotonic() + 5
                while not (root / 'entered').exists() and time.monotonic() < deadline:
                    time.sleep(.01)
                assert (root / 'entered').exists(), 'held control hook did not start'
                marker.write_text('')
                rpc.call('thread/resume', {'threadId': plain, 'config': {key: False}})
                rpc.call('thread/settings/update', {'threadId': plain, 'disabledPluginIds': ['activity-probe@activity-probe']})
                (root / 'release').touch()
                deadline = time.monotonic() + 5
                while not events(plain) and time.monotonic() < deadline:
                    time.sleep(.01)
                late_events = events(plain)
                rpc.call('turn/interrupt', {'threadId': plain, 'turnId': turn})
                summary = {
                    'version': subprocess.check_output(['codex', '--version'], text=True).strip(),
                    'new_thread_config_suppression': True,
                    'loaded_resume_still_emits': resume_events,
                    'settings_suppresses_subsequent_hooks_only': True,
                    'unmanaged_thread_unaffected': True,
                    'in_flight_hook_emitted_after_settings_response': late_events,
                }
                print(json.dumps(summary, indent=2))
                assert 'UserPromptSubmit' in late_events, 'in-flight blocker no longer reproduced'
                print('BLOCKER reproduced: attaching to an active thread cannot fence its already-running producer hook.')
            finally:
                rpc.close()
                if __import__('sys').exc_info()[0]:
                    print((root / 'server.log').read_text()[-5000:])


if __name__ == '__main__':
    probe(coordinated='--coordinated' in sys.argv)
