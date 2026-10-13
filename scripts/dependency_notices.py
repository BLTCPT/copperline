#!/usr/bin/env python3
"""Generate license texts from the exact resolved Cargo package source trees."""
import json
from pathlib import Path
import subprocess
m=json.loads(subprocess.check_output(['cargo','metadata','--locked','--format-version','1'],text=True))
output=[]
for p in sorted(m['packages'],key=lambda p:p['name']):
    if p['source'] is None:
        continue
    base=Path(p['manifest_path']).parent
    files=[f for f in base.iterdir() if f.is_file() and f.name.upper().startswith(('LICENSE','COPYING','NOTICE'))]
    output.append('\n'+'='*72+'\n'+p['name']+' '+p['version']+' | '+str(p.get('license'))+'\n'+str(p.get('repository'))+'\n')
    if not files:
        output.append('License expression: '+str(p.get('license'))+'; source crate distributed via crates.io.\n')
    if p['name']=='epaint_default_fonts':
        files.extend(f for f in base.rglob('*') if f.is_file() and ('license' in f.name.lower() or 'copyright' in f.name.lower()) and f not in files)
    for f in sorted(files):
        try:
            output.append('\n'+str(f.relative_to(base))+'\n'+f.read_text())
        except UnicodeError:
            output.append(f.name+' (binary notice; consult source crate)\n')
Path('legal/THIRD_PARTY_NOTICES.txt').write_text('Copperline third-party source license notices\nGenerated from Cargo.lock packages, all platforms; no vendor application assets.\n'+''.join(output))
print('Wrote notices for',sum(p['source'] is not None for p in m['packages']),'dependencies')
