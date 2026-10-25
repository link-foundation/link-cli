"""Run the public-API probe with a finite deadline and child resource limits."""
from pathlib import Path
import resource
import subprocess
base=Path('experiments/issue-138')
def limits():
    resource.setrlimit(resource.RLIMIT_AS,(512*1024*1024,512*1024*1024))
    resource.setrlimit(resource.RLIMIT_STACK,(8*1024*1024,8*1024*1024))
result=subprocess.run([str(base/'recovery-contradiction-probe')],stdout=subprocess.PIPE,
    stderr=subprocess.STDOUT,text=True,timeout=10,preexec_fn=limits)
print(result.stdout,end='')
raise SystemExit(result.returncode)
