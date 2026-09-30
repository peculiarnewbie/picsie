"""Estimate drag position lag from the recorded triangular motion and blue edge.

One detector pixel corresponds to ~8.3 ms of motion in the faster drag. These
estimates are separate from the directly observed keyboard latency measurements.
"""
import argparse
import json
from pathlib import Path
import statistics

parser = argparse.ArgumentParser()
parser.add_argument('report', type=Path)
args = parser.parse_args()
report = json.loads(args.report.read_text())
values = {}
details = []
for run in report['runs']:
    name = run.get('variant', run['app'])
    drag = run['drags'][1]
    previous = run['initial_edge_x']
    samples = []
    for observed, position in drag['visible_changes']:
        offset = position-run['initial_edge_x']
        forward = position > previous
        previous = position
        index = round(offset*240/drag['excursion_screen_px'] if forward else
                      480-offset*240/drag['excursion_screen_px'])
        if not 1 <= index <= len(drag['input_times_s']):
            raise ValueError((name, observed, position, index))
        samples.append((observed-drag['input_times_s'][index-1])*1000)
    values.setdefault(name, []).extend(samples)
    details.append({'variant':name,'trial':run['trial'],'samples_ms':samples})
summary = {}
for name, samples in values.items():
    samples.sort()
    summary[name] = {'median_ms':statistics.median(samples),
                     'min_ms':min(samples),'max_ms':max(samples),'count':len(samples)}
result = {'method':'Match the changed blue edge offset to its scripted triangular motion input; infer direction from consecutive positions',
          'scope':'Estimated continuous-drag position lag; one screen pixel ~8.3 ms, plus observation delay; not keyboard latency',
          'summary':summary, 'runs':details}
args.report.with_name('position-lag.json').write_text(json.dumps(result,indent=2)+'\n')
print(json.dumps(summary,indent=2))
