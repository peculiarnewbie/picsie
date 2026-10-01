"""Casewise native stress summaries, including validated rows before a failure.

Protocol cohorts remain separate. This does not repair failed launches, pool
different views, estimate physical FPS, or discard slow samples for contention.
"""
import argparse
from collections import Counter, defaultdict
import hashlib
import json
from pathlib import Path
import statistics


def summarize(values):
    return dict(n=len(values), median=statistics.median(values),
                minimum=min(values), maximum=max(values))


def cohort(path):
    manifest = json.loads((path / 'manifest.json').read_text())
    runs = []
    grouped = defaultdict(list)
    for folder in sorted(path.iterdir()):
        if not folder.is_dir() or not folder.name.startswith(('picsie-', 'gimp-')):
            continue
        result_path = folder / 'result.json'
        partial_path = folder / 'samples.partial.json'
        if result_path.exists():
            run = json.loads(result_path.read_text())
            rows = run['results']
            app, key, trial = run['app'], run['key'], run['trial']
            status = run['status']
        elif partial_path.exists():
            app, remainder = folder.name.split('-', 1)
            # A retained interrupted folder can have a .failed-N suffix.
            key, trial_text = remainder.split('.failed-', 1)[0].rsplit('-', 1)
            trial = int(trial_text)
            run = {}
            rows = json.loads(partial_path.read_text())
            status = 'incomplete'
        else:
            continue
        if trial < 1:
            continue
        launch = dict(folder=folder.name, app=app, key=key, trial=trial,
                      status=status, observations=len(rows))
        for field in ('error', 'application_exit_code', 'host_before', 'host_after'):
            if field in run:
                launch[field] = run[field]
        runs.append(launch)
        for row in rows:
            grouped[app, key, row['case']].append((folder.name, status, row))
    cases = {}
    for (app, key, case), samples in sorted(grouped.items()):
        metrics = {}
        for metric in ('first_visible_ms', 'final_visible_ms', 'endpoint_ms',
                       'updates_per_second', 'undo_final_visible_ms',
                       'cpu_seconds', 'rss_mib'):
            values = [row[metric] for _, _, row in samples
                      if row.get(metric) is not None]
            if values:
                metrics[metric] = summarize(values)
        intervals = [value for _, _, row in samples
                     for value in row.get('intervals_ms', [])]
        if intervals:
            metrics['visible_interval_ms'] = summarize(intervals)
        cases.setdefault(app, {}).setdefault(key, {})[case] = dict(
            observations=len(samples), launches=len({s[0] for s in samples}),
            observation_status_counts=dict(Counter(status for _, status, _ in samples)),
            absent_first_response=sum(row.get('first_visible_ms') is None
                                      for _, _, row in samples),
            metrics=metrics)
    return dict(manifest=manifest, launch_status_counts=dict(Counter(
        run['status'] for run in runs)), completed_observations=sum(
        run['observations'] for run in runs), runs=runs, cases=cases)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('cohorts', type=Path, nargs='+')
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    report = dict(scope=__doc__, source_sha256=hashlib.sha256(
        Path(__file__).read_bytes()).hexdigest(), cohorts={
            str(path): cohort(path) for path in args.cohorts})
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, indent=2) + '\n')
    for path, result in report['cohorts'].items():
        print(path, result['launch_status_counts'],
              result['completed_observations'], 'completed observations')


if __name__ == '__main__':
    main()
