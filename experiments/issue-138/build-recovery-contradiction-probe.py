"""Build the finite public-API probe from the repository root after cargo test."""
from pathlib import Path
import subprocess
base=Path('experiments/issue-138')
lib=max(Path('rust/target/debug/deps').glob('liblink_cli-*.rlib'),key=lambda p:p.stat().st_mtime)
command=['rustc','--edition=2021','-C','debuginfo=0','-L','dependency=rust/target/debug/deps','--extern',f'link_cli={lib}',str(base/'recovery-contradiction-probe.rs'),'-o',str(base/'recovery-contradiction-probe')]
for directory in Path('rust/target/debug/build').glob('ring-*/out'):
    command.extend(['-L',f'native={directory}'])
print('Building finite journal contradiction probe against',lib,flush=True)
subprocess.run(command,check=True)
