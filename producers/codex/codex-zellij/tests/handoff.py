#!/usr/bin/env python3
"""End-to-end producer handoff regression. Run after cargo build."""
import json
import os
from pathlib import Path
import subprocess
import select
import tempfile
import time

ROOT = Path(__file__).resolve().parents[4]
FORWARDER = ROOT / 'producers/codex/scripts/forwarder.sh'
OWNER = ROOT / 'producers/codex/codex-zellij/target/debug/examples/hold-lease'


def until(predicate, message):
    deadline = time.monotonic() + 6
    while time.monotonic() < deadline:
        if predicate():
            return
        time.sleep(.01)
    raise AssertionError(message)


def test():
    with tempfile.TemporaryDirectory(prefix='zca-handoff-', dir='/tmp') as temporary:
        root = Path(temporary)
        coordination = root / 'coordination'
        coordination.mkdir(mode=0o700)
        log = root / 'sent'
        script = root / 'zellij'
        # Synchronize a real already-running forwarder at its actual Zellij subprocess.
        script.write_text('#!/usr/bin/env python3\nimport pathlib,sys,time\n'
                          f'p=pathlib.Path({str(root)!r})\n'
                          'if (p/"hold").exists():\n'
                          ' (p/"entered").touch()\n'
                          ' while not (p/"release").exists(): time.sleep(.01)\n'
                          'with (p/"sent").open("a") as f: f.write(" ".join(sys.argv[1:])+"\\n")\n')
        script.chmod(0o700)
        env = dict(os.environ, PATH=f'{root}:'+os.environ['PATH'],
                   ZELLIJ_SESSION_NAME='legacy', ZELLIJ_PANE_ID='26',
                   ZELLIJ_AGENT_ACTIVITY_COORDINATION_DIR=str(coordination))
        env.pop('ZELLIJ_AGENT_ACTIVITY_DRY_RUN', None)
        env.pop('ZELLIJ_AGENT_ACTIVITY_LOG', None)
        def hook(thread):
            return subprocess.Popen(['bash', str(FORWARDER)], env=env, text=True,
                                    stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        def send(thread):
            process=hook(thread)
            process.communicate(json.dumps({'session_id':thread,'hook_event_name':'Stop'}),timeout=5)
            assert process.returncode==0
        (root/'hold').touch()
        old=hook('thread-a')
        old.stdin.write(json.dumps({'session_id':'thread-a','hook_event_name':'Stop'}))
        old.stdin.close()
        until(lambda:(root/'entered').exists(), 'old hook never reached sink')
        # Test-only lease driver exercises the same ownership implementation as the proxy sink.
        owner=subprocess.Popen([str(OWNER),str(coordination),'thread-a'],
                               stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True)
        try:
            assert owner.stdout.readline().strip()=='acquiring'
            assert not select.select([owner.stdout],[],[],.1)[0], 'proxy acquired before old hook drained'
            (root/'release').touch()
            old.wait(timeout=5)
            assert owner.stdout.readline().strip()=='owned', owner.stderr.read() if owner.poll() is not None else 'no lease'
            before=log.read_text().splitlines()
            send('thread-a')
            assert log.read_text().splitlines()==before, 'managed conversation still emits a hook'
            send('thread-b')
            assert len(log.read_text().splitlines())==len(before)+1, 'unmanaged conversation was suppressed'
            second=subprocess.Popen([str(OWNER),str(coordination),'thread-a'],
                                    stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True)
            try:
                assert second.stdout.readline().strip()=='acquiring'
                assert second.stdout.readline().strip()=='owned'
                owner.kill(); owner.wait(timeout=5)
                before=log.read_text()
                send('thread-a')
                assert log.read_text()==before, 'closing one view restored hooks while another remains'
            finally:
                second.kill(); second.wait(timeout=5)
            send('thread-a')
            assert len(log.read_text().splitlines())==len(before.splitlines())+1, 'last-view crash did not restore hooks'
            print('PASS: in-flight hook drained; managed suppressed; unrelated preserved; multi-view and crash cleanup restored.')
        finally:
            if owner.poll() is None: owner.kill(); owner.wait(timeout=5)
            if old.poll() is None: old.kill(); old.wait(timeout=5)


if __name__=='__main__':
    test()
