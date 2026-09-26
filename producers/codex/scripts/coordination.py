#!/usr/bin/env python3
"""Protocol 1: serialize legacy emission with per-conversation proxy ownership."""
import fcntl
import json
import os
from pathlib import Path
import stat
import subprocess
import sys


def main():
    body = sys.stdin.read()
    try:
        thread = json.loads(body).get('session_id')
    except (ValueError, AttributeError):
        return 0
    if not isinstance(thread, str) or not thread or len(thread.encode()) > 100:
        # Old fixtures/harnesses lacking identity keep their legacy behavior.
        return subprocess.run(
            ['bash', sys.argv[1], '--coordinated'], input=body, text=True).returncode
    root = Path(os.environ.get('ZELLIJ_AGENT_ACTIVITY_COORDINATION_DIR',
                               str(Path.home()/'.local/state/zellij-agent-activity/codex-v1')))
    root.mkdir(parents=True, exist_ok=True, mode=0o700)
    info = root.lstat()
    if not stat.S_ISDIR(info.st_mode) or info.st_uid != os.getuid() or info.st_mode & 0o077:
        return 0
    key = thread.encode().hex()
    flags = os.O_RDWR | os.O_CREAT | os.O_NOFOLLOW
    gate = os.open(root/(key+'.gate'), flags, 0o600)
    lease = os.open(root/(key+'.owners'), flags, 0o600)
    try:
        fcntl.flock(gate, fcntl.LOCK_EX)
        try:
            fcntl.flock(lease, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError:
            return 0  # At least one proxy view owns this conversation.
        # Inherit locks into the emitting shell: killing this wrapper must not release
        # ownership while a child can still write to Zellij. Never unlink lock files.
        return subprocess.run(['bash', sys.argv[1], '--coordinated'], input=body,
                              text=True, pass_fds=(gate, lease)).returncode
    finally:
        os.close(lease)
        os.close(gate)


if __name__ == '__main__':
    try:
        sys.exit(main())
    except OSError:
        # Failing closed keeps coordination errors from creating duplicate producers.
        sys.exit(0)
