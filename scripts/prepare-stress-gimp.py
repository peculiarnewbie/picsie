"""Prepare validated native GIMP fixtures outside all measured workloads.

First use --compare-ora --filter 1200-50- on the small simple/complex controls.
The adapter checks their complete merged RGBA bytes and hierarchy against ORA.
Then prepare all fixtures; every XCF round-trip checks hierarchy and full pixels.
"""
import argparse
import importlib.util
import os
from pathlib import Path
import subprocess
import sys

sys.dont_write_bytecode=True
ROOT=Path(__file__).resolve().parent.parent
spec=importlib.util.spec_from_file_location('comparison',ROOT/'scripts/compare-gimp-performance.py')
comparison=importlib.util.module_from_spec(spec);spec.loader.exec_module(comparison)

def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('--output',type=Path,required=True)
    p.add_argument('--fixtures',type=Path,default=ROOT/'artifacts/perf-stress-2026-10-01/fixtures')
    p.add_argument('--runtime',type=Path,default=ROOT/'artifacts/gimp-performance/runtime')
    p.add_argument('--filter',default='');p.add_argument('--compare-ora',action='store_true')
    p.add_argument('--cache-mib',type=int,default=256,help='Preparation-only tile cache; measured apps retain their normal configuration')
    a=p.parse_args()
    for key in ['output','fixtures','runtime']:setattr(a,key,getattr(a,key).resolve())
    a.output.mkdir(parents=True,exist_ok=False)
    config=a.output/'gimp-config';config.mkdir()
    assert a.cache_mib>0
    (config/'gimprc').write_text(f'(show-welcome-dialog no)\n(check-updates no)\n(use-opencl no)\n(tile-cache-size {a.cache_mib}M)\n')
    env={**os.environ,**comparison.runtime_environment(a.runtime),
        'PATH':str(a.runtime/'usr/bin')+os.pathsep+os.environ.get('PATH',''),
        'GIMP3_DIRECTORY':str(config),'GIMP3_CACHEDIR':str(a.output/'gimp-cache'),
        'PICSIE_STRESS_FIXTURES':str(a.fixtures),'PICSIE_STRESS_OUTPUT':str(a.output),
        'PICSIE_STRESS_FILTER':a.filter,'PICSIE_STRESS_COMPARE_ORA':'1' if a.compare_ora else '0',
        'GEGL_USE_OPENCL':'no'}
    workload=ROOT/'scripts/perf/gimp-stress-prepare.py'
    command=[str(a.runtime/'usr/bin/gimp-console'),'--new-instance','--no-interface',
        '--batch-interpreter=python-fu-eval','--batch',
        f"exec(compile(open({str(workload)!r}).read(),'prepare-stress','exec'), {{'__file__': {str(workload)!r}}})",'--quit']
    with (a.output/'app.log').open('w') as log:
        subprocess.run(command,env=env,stdout=log,stderr=subprocess.STDOUT,check=True,timeout=7200)

if __name__=='__main__':main()
