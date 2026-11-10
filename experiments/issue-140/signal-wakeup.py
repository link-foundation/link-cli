"""Bounded signal-wakeup regression; the previous Event handler deadlocked.

Run: PYTHONPATH=ipc python experiments/issue-140/signal-wakeup.py
Only 1000 notifications are sent, with a ten-second diagnostic watchdog.
"""

import faulthandler
import os
import signal

from link_ipc.local import enable_signals, wait_tick

faulthandler.dump_traceback_later(10, exit=True)
enable_signals()
for _ in range(1000):
    os.kill(os.getpid(), signal.SIGUSR1)
    wait_tick(0.001)
faulthandler.cancel_dump_traceback_later()
print("1000 wakeups completed")
