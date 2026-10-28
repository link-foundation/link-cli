#!/usr/bin/env python3
"""Report exact-head C# archives, preserving aggregate and source-line counters."""
from pathlib import Path
import hashlib
import json
import re
import zipfile
import xml.etree.ElementTree as ET

ROOT=Path(__file__).resolve().parents[2]
BASE=ROOT/'experiments/issue-138'
HEAD='80c41ad3196cb87f4f5291eb16e459bbd741afce'
metadata_path=BASE/'causal-batch-csharp-artifacts-current.json'
metadata=json.loads(metadata_path.read_text())
print('PR: https://github.com/link-foundation/link-cli/pull/139')
print('Exact head:',HEAD)
print('C# run: https://github.com/link-foundation/link-cli/actions/runs/40558747229')
for platform,artifact,job in [('linux',12757789878,122382118519),('macos',12758846294,122382118629),('windows',12760910361,122382118801)]:
    path=BASE/f'ci-logs/causal-batch-csharp-{platform}-{artifact}.zip'
    data=path.read_bytes()
    record,=[entry for entry in metadata['artifacts'] if entry['id']==artifact]
    assert record['workflow_run']['id']==40558747229
    assert record['workflow_run']['head_sha']==HEAD
    assert record['workflow_run']['head_branch']=='issue-138-adc2aa56c4cb'
    assert record['size_in_bytes']==len(data)
    assert record['digest']=='sha256:'+hashlib.sha256(data).hexdigest()
    print(f'\n{platform}: https://github.com/link-foundation/link-cli/actions/runs/40558747229/job/{job}')
    print(f'{path.relative_to(ROOT)}: {len(data)} bytes; SHA256 {hashlib.sha256(data).hexdigest()}.')
    with zipfile.ZipFile(path) as archive:
        name,=[name for name in archive.namelist() if name.endswith('coverage.cobertura.xml')]
        xml=archive.read(name);report=ET.fromstring(xml)
    print(f'{name}: {len(xml)} bytes; SHA256 {hashlib.sha256(xml).hexdigest()}.')
    for metric in ('lines','branches'):
        covered,total=[int(report.attrib[f'{metric}-{k}']) for k in ('covered','valid')]
        print(f'Complete {metric}: {covered}/{total} ({100*covered/total:.5f}%).')
    for filename in ['CausalVersion.cs','CausalReplica.cs','CausalMerkle.cs','CausalBatch.cs']:
        classes=[c for c in report.findall('.//class') if c.attrib['filename'].endswith('/'+filename)]
        assert classes,filename
        lines=[line for c in classes for line in c.findall('./lines/line')]
        all_lines={int(line.attrib['number']) for line in lines}
        hit_lines={int(line.attrib['number']) for line in lines if int(line.attrib['hits'])>0}
        branches=[tuple(map(int,re.search(r'\((\d+)/(\d+)\)',line.attrib['condition-coverage']).groups())) for line in lines if 'condition-coverage' in line.attrib]
        print(f'{filename}: distinct source lines {len(hit_lines)}/{len(all_lines)}; source-line branch records {sum(c for c,n in branches)}/{sum(n for c,n in branches)}.')
        for cls in classes:
            if cls.attrib['branch-rate']!='1':
                print(f"Class aggregate {cls.attrib['name']}: branch-rate={cls.attrib['branch-rate']}; complexity={cls.attrib['complexity']}.")
print('\nClass aggregate counters can include methods without source lines that the Cobertura source-line listing omits.')
print('Source-line branch sums alone do not prove complete compiled branch coverage. Raw JSON investigation remains active; all whole-project native/C#/WASM gaps remain unfinished.')
print('These immutable archives cover the published batch head, before the causal projection changes.')
data=metadata_path.read_bytes()
print(f'{metadata_path.relative_to(ROOT)}: {len(data)} bytes; SHA256 {hashlib.sha256(data).hexdigest()}.')
