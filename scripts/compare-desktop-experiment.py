"""Paired desktop experiments using the GIMP-comparison measurement loop.

Requires the staged tools/runtime/fixtures in docs/gimp-performance.md.
No app tracing is enabled. Results end at the virtual X11 framebuffer.
"""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import re
import sys

sys.dont_write_bytecode = True
root = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('comparison', root/'scripts/compare-gimp-performance.py')
c = importlib.util.module_from_spec(spec)
spec.loader.exec_module(c)
parser = argparse.ArgumentParser()
parser.add_argument('--before', type=Path, required=True)
parser.add_argument('--after', type=Path, required=True)
parser.add_argument('--output', type=Path, required=True)
parser.add_argument('--trials', type=int, default=3)
parser.add_argument('--jitter-nudges', action='store_true',
                    help='Use reproducible varied idle gaps to sample more refresh phases')
parser.add_argument('--gimp', action='store_true')
parser.add_argument('--before-env', action='append', default=[], metavar='KEY=VALUE')
parser.add_argument('--after-env', action='append', default=[], metavar='KEY=VALUE')
args = parser.parse_args()
if args.trials < 1:
    parser.error('--trials must be at least 1')
variant_env = {}
for name in ['before', 'after']:
    variant_env[name] = {}
    for value in getattr(args, name+'_env'):
        key, separator, setting = value.partition('=')
        if not separator or not re.fullmatch('[A-Z][A-Z0-9_]*', key):
            parser.error('environment overrides must be KEY=VALUE with an uppercase key')
        variant_env[name][key] = setting
args.runtime = root/'artifacts/gimp-performance/runtime'
args.fixture = root/'artifacts/gimp-performance/fixture'
args.display = ':96'
args.before, args.after, args.output = args.before.resolve(), args.after.resolve(), args.output.resolve()
args.output.mkdir(parents=True, exist_ok=True)
tools = root/'artifacts/selection-history/tools/usr'
env = dict(os.environ, DISPLAY=args.display, WINIT_UNIX_BACKEND='x11',
           XDG_CACHE_HOME=str(args.output/'cache'),
           PATH=str(tools/'bin')+os.pathsep+os.environ.get('PATH', ''),
           LD_LIBRARY_PATH=str(tools/'lib'),
           VK_DRIVER_FILES=str(tools/'share/vulkan/icd.d/lvp_icd.json'))
env.pop('WAYLAND_DISPLAY', None)
# A comparison's experiment switches must come from its explicit per-variant
# overrides, not from the caller's shell or an earlier diagnostic launch.
for key in list(env):
    if key.startswith('PICSIE_'):
        env.pop(key)
applications = ['before', 'after']+(['gimp'] if args.gimp else [])
manifest = {
    'scope': 'Untraced X11/Xvfb layer move; same GIMP-comparison input and observation loops',
    'warmup': 'One full discarded run per variant; rotating launch order',
    'trials': args.trials,
    'nudge_spacing': 'Seeded 80–147 ms gaps, seed 7391 + trial' if args.jitter_nudges else '100 ms after each visible change',
    'variant_env': variant_env,
    'binary_paths': {'before': str(args.before), 'after': str(args.after)},
    'driver_sha256': {p.name: hashlib.sha256(p.read_bytes()).hexdigest() for p in
                      [Path(__file__), root/'scripts/compare-gimp-performance.py', root/'scripts/compare-desktop-performance.py']},
    'binary_sha256': {name: hashlib.sha256(path.read_bytes()).hexdigest() for name, path in
                      [('before', args.before), ('after', args.after)]},
}
# Preserve provenance even if an interaction assertion stops this cohort.
(args.output/'manifest.json').write_text(json.dumps(manifest, indent=2)+'\n')
runs = []
for trial in range(args.trials+1):
    order = applications[trial % len(applications):]+applications[:trial % len(applications)]
    for name in order:
        folder = args.output/name
        folder.mkdir(exist_ok=True)
        args.picsie = args.before if name == 'before' else args.after
        launch_env = dict(env)
        for key, value in variant_env.get(name, {}).items():
            if value:
                launch_env[key] = value
            else:
                launch_env.pop(key, None)
        try:
            result = c.run_one('gimp' if name == 'gimp' else 'picsie', trial, args, launch_env, folder)
        except Exception as error:
            (args.output/'failure.json').write_text(json.dumps({
                'variant': name, 'trial': trial, 'error': str(error),
                'completed_measured_runs': len(runs),
            }, indent=2)+'\n')
            raise
        result['variant'] = name
        if trial:
            runs.append(result)
summary = {}
for name in applications:
    measured = [r for r in runs if r['variant'] == name]
    summary[name] = {
        'nudge_latency_ms':c.desktop.stats([v for r in measured for v in r['nudge_samples_ms']]),
        'drag_updates_per_second':c.desktop.stats([r['drags'][1]['visible_updates_per_second'] for r in measured]),
        'drag_visible_interval_ms':c.desktop.stats([v for r in measured for v in
            [(b[0]-a[0])*1000 for a,b in zip(r['drags'][1]['visible_changes'],r['drags'][1]['visible_changes'][1:])]]),
        'drag_cpu_percent_one_core':c.desktop.stats([r['drags'][1]['cpu_percent_one_core'] for r in measured]),
        'idle_rss_mib':c.desktop.stats([r['idle']['rss_mib'] for r in measured])}
report = {**manifest, 'summary': summary, 'runs': runs}
(args.output/'application-results.json').write_text(json.dumps(report,indent=2)+'\n')
print(json.dumps(summary,indent=2),flush=True)
