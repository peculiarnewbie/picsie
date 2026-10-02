"""Run sequential repeated CPU stress workloads and retain every sample.

Use --prepare after building performance_stress. Native navigation is measured
separately: GIMP public API thumbnails are not equivalent viewport previews.
"""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys
import time
import xml.etree.ElementTree as ET
import zipfile

sys.dont_write_bytecode=True
ROOT=Path(__file__).resolve().parent.parent
spec=importlib.util.spec_from_file_location('comparison',ROOT/'scripts/compare-gimp-performance.py')
comparison=importlib.util.module_from_spec(spec);spec.loader.exec_module(comparison)

def package(fixtures):
    for spec in json.loads((fixtures/'fixtures.json').read_text()):
        key=spec['key'];folder=fixtures/key
        stamp=fixtures/(key+'.ora.json')
        asset_hash=hashlib.sha256((folder/'layers.json').read_bytes()+(folder/'0.png').read_bytes()).hexdigest()
        if stamp.exists() and json.loads(stamp.read_text())==dict(revision=3,layers_sha256=asset_hash):
            continue
        records=json.loads((folder/'layers.json').read_text())
        image=ET.Element('image',{'w':str(spec['width']),'h':str(spec['height']),'name':key})
        root=ET.SubElement(image,'stack');groups={}
        with zipfile.ZipFile(fixtures/(key+'.ora'),'w',compression=zipfile.ZIP_DEFLATED) as archive:
            archive.writestr('mimetype','image/openraster',compress_type=zipfile.ZIP_STORED)
            # Parent nodes are created before children. Top-to-bottom ORA order
            # is then obtained by reversing each stack, including nested stacks.
            for record in records:
                parent=groups.get(record['parent'],root)
                if record['group']:
                    groups[record['id']]=ET.SubElement(parent,'stack',{'name':record['name'],'opacity':str(record['opacity'])})
                else:
                    file='data/'+record['file']
                    ET.SubElement(parent,'layer',{'name':record['name'],'src':file,
                        'x':str(round(record['x'])),'y':str(round(record['y'])),
                        'opacity':str(record['opacity']),'visibility':'visible',
                        'composite-op':record['blend'],
                        'selected':'true' if record['id']==records[-1]['id'] else 'false'})
                    archive.write(folder/record['file'],file)
            for stack in image.iter('stack'): stack[:]=list(reversed(list(stack)))
            archive.writestr('stack.xml',ET.tostring(image))
            archive.write(folder/'merged.png','mergedimage.png')
        stamp.write_text(json.dumps(dict(revision=3,layers_sha256=asset_hash))+'\n')

def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('--output',type=Path,required=True)
    p.add_argument('--fixtures',type=Path,default=Path('artifacts/perf-stress-2026-10-01/fixtures'))
    p.add_argument('--binary',type=Path,default=Path('target/release/examples/performance_stress'))
    p.add_argument('--runtime',type=Path,default=Path('artifacts/gimp-performance/runtime'))
    p.add_argument('--prepare',action='store_true')
    p.add_argument('--mode',choices=['stacks','resize'],default='stacks')
    p.add_argument('--gimp-format',choices=['ora','xcf'],default='ora',help='XCF requires separately validated preparation; changes setup only')
    p.add_argument('--apps',nargs='+',choices=['picsie','gimp'],default=['picsie','gimp'])
    p.add_argument('--trials',type=int,default=3);p.add_argument('--samples',type=int,default=4)
    p.add_argument('--warmups',type=int,default=2);p.add_argument('--filter',default='')
    p.add_argument('--bounded',action='store_true',help='Large complex fixtures: adjacent reorder only, all navigation; full reorder variants on moderate canvas')
    p.add_argument('--cases',default='',help='Comma separated exact case names; empty runs all')
    a=p.parse_args()
    for key in ['output','fixtures','binary','runtime']:setattr(a,key,getattr(a,key).resolve())
    if a.output.exists():p.error('Choose a new output directory')
    a.output.mkdir(parents=True)
    if a.prepare:
        subprocess.run([str(a.binary),'prepare',str(a.fixtures)],check=True)
    package(a.fixtures)
    workload=ROOT/'scripts/perf/gimp-stress-workload.py'
    paths=[Path(__file__),workload,ROOT/'scripts/prepare-stress-gimp.py',ROOT/'scripts/perf/gimp-stress-prepare.py',ROOT/'crates/picsie-core/examples/performance_stress.rs',ROOT/'scripts/compare-gimp-performance.py',ROOT/'scripts/compare-desktop-performance.py',a.binary,a.runtime/'usr/bin/gimp-console']
    manifest=dict(head=subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip(),
        mode=a.mode,apps=a.apps,trials=a.trials,samples=a.samples,warmups=a.warmups,filter=a.filter,cases=a.cases,bounded=a.bounded,gimp_format=a.gimp_format,
        source_sha256={(str(path.relative_to(ROOT)) if path.is_relative_to(ROOT) else str(path)):hashlib.sha256(path.read_bytes()).hexdigest() for path in paths},
        fixture_sha256={path.name:hashlib.sha256(path.read_bytes()).hexdigest() for path in a.fixtures.iterdir() if path.is_file() and path.suffix in ['.picsie','.ora','.xcf','.json']},
        methods='Sequential alternating apps; fresh editor/image each sample. Setup/assertions excluded. No live input or UI/GPU timing. GIMP complex controls bake masks/affine/adjustments and omit live clipping. Preserve all measured samples.')
    (a.output/'manifest.json').write_text(json.dumps(manifest,indent=2)+'\n')
    (a.output/'working-tree.patch').write_bytes(subprocess.check_output(['git','diff']))
    all_results=[]
    for trial in range(1,a.trials+1):
        for app in (a.apps if trial%2 else list(reversed(a.apps))):
            folder=a.output/f'{app}-{trial}';folder.mkdir()
            env={**os.environ,**comparison.runtime_environment(a.runtime),
                 'PATH':str(a.runtime/'usr/bin')+os.pathsep+os.environ.get('PATH',''),
                 'PICSIE_STRESS_FIXTURES':str(a.fixtures),'PICSIE_STRESS_OUTPUT':str(folder),
                 'PICSIE_STRESS_WARMUPS':str(a.warmups),'PICSIE_STRESS_SAMPLES':str(a.samples),
                 'PICSIE_STRESS_MODE':a.mode,'PICSIE_STRESS_FILTER':a.filter,'PICSIE_STRESS_CASES':a.cases,
                 'PICSIE_STRESS_GIMP_FORMAT':a.gimp_format,
                 'PICSIE_STRESS_BOUNDED':'1' if a.bounded else '0',
                 'PICSIE_STRESS_PROBES':'1' if trial==1 else '0','GEGL_USE_OPENCL':'no'}
            config=folder/'gimp-config';config.mkdir()
            (config/'gimprc').write_text('(show-welcome-dialog no)\n(check-updates no)\n(use-opencl no)\n')
            env.update(GIMP3_DIRECTORY=str(config),GIMP3_CACHEDIR=str(folder/'gimp-cache'))
            if app=='picsie': command=[str(a.binary),'run',str(a.fixtures),str(folder),str(a.warmups),str(a.samples),a.mode]
            else: command=[str(a.runtime/'usr/bin/gimp-console'),'--new-instance','--no-interface','--batch-interpreter=python-fu-eval','--batch',f"exec(compile(open({str(workload)!r}).read(),'stress','exec'))",'--quit']
            before=comparison.host_resources()
            with (folder/'app.log').open('w') as log:
                process=subprocess.Popen(command,env=env,stdout=log,stderr=subprocess.STDOUT,start_new_session=True)
                peak_rss_mib=0.;process_cpu_seconds=0.;deadline=time.monotonic()+7200
                try:
                    while True:
                        try:
                            resources=comparison.desktop.resources(process.pid)
                            peak_rss_mib=max(peak_rss_mib,resources['rss_mib'])
                            process_cpu_seconds=max(process_cpu_seconds,resources['cpu_seconds'])
                        except FileNotFoundError:pass
                        try:code=process.wait(timeout=.25);break
                        except subprocess.TimeoutExpired:
                            if time.monotonic()>deadline:raise
                finally:comparison.desktop.stop(process)
            assert code==0,f'{app} failed: {folder}/app.log'
            result=json.loads((folder/'results.json').read_text());assert result['results'],'No selected workload cases executed';result.update(trial=trial,host_before=before,host_after=comparison.host_resources(),process_peak_rss_mib=peak_rss_mib,process_cpu_seconds=process_cpu_seconds)
            all_results.append(result)
            print(f'{app}-{trial}: {len(result["results"])} completed cases',flush=True)
    summary={}
    for app in a.apps:
        grouped={}
        for run in all_results:
            if run['app']!=app:continue
            for row in run['results']:
                grouped.setdefault(row['key']+'/'+row['case'],[]).extend(row['samples'])
        summary[app]={key:{metric:comparison.desktop.stats([s[metric] for s in samples if s[metric] is not None]) for metric in samples[0] if samples[0][metric] is not None} for key,samples in grouped.items()}
    (a.output/'results.json').write_text(json.dumps(dict(manifest=manifest,summary=summary,runs=all_results),indent=2)+'\n')

if __name__=='__main__':main()
