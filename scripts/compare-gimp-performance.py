"""Measure GIMP and Picsie through XTest input and XGetImage canvas observations.

Requires the runtime and six-layer fixtures described in docs/gimp-performance.md.
Reuses the desktop comparison's external detector, resource sampler, and statistics.
"""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import random
import statistics
import subprocess
import sys
import time
import xml.etree.ElementTree as ET
import zipfile

sys.dont_write_bytecode = True
spec = importlib.util.spec_from_file_location(
    "desktop_benchmark", Path(__file__).with_name("compare-desktop-performance.py"))
desktop = importlib.util.module_from_spec(spec)
spec.loader.exec_module(desktop)


def host_resources():
    """Record host contention alongside app results without excluding samples."""
    if not Path('/proc/pressure').exists():
        return None
    pressure = {}
    for resource in ['cpu', 'memory', 'io']:
        pressure[resource] = {}
        for line in Path('/proc/pressure', resource).read_text().splitlines():
            kind, *values = line.split()
            pressure[resource][kind] = {
                key: float(value) for key, value in (item.split('=') for item in values)
            }
    memory = {}
    for line in Path('/proc/meminfo').read_text().splitlines():
        key, value = line.split(':', 1)
        if key in ['MemAvailable', 'SwapTotal', 'SwapFree']:
            memory[key+'_kib'] = int(value.split()[0])
    return {'monotonic_s': time.monotonic(), 'pressure': pressure, 'memory': memory,
            'load_average': list(os.getloadavg())}


def runtime_environment(root):
    base = root.parent
    directories = {'GIMP3_DIRECTORY': base/'gimp-config',
                   'GIMP3_CACHEDIR': base/'gimp-cache', 'GIMP3_TEMPDIR': base/'tmp'}
    for directory in directories.values():
        directory.mkdir(parents=True, exist_ok=True)
    return {**{name: str(path) for name, path in directories.items()},
            'LD_LIBRARY_PATH': str(root/'usr/lib'),
            'GI_TYPELIB_PATH': str(root/'usr/lib/girepository-1.0'),
            'BABL_PATH': str(root/'usr/lib/babl-0.1'), 'GEGL_PATH': str(root/'usr/lib/gegl-0.4'),
            'GIMP3_DATADIR': str(root/'usr/share/gimp/3.0'),
            'GIMP3_PLUGINDIR': str(root/'usr/lib/gimp/3.0'),
            'GIMP3_SYSCONFDIR': str(root/'etc/gimp/3.0'),
            'XDG_DATA_DIRS': str(root/'usr/share')+':/usr/local/share:/usr/share'}


def prepare_fixture(folder):
    """Ask the Rust engine to render layer assets; package them as OpenRaster."""
    folder.mkdir(parents=True, exist_ok=True)
    subprocess.run(['node', '-e', """
const {NativeEditor, openEditor} = require('./native/picsie.node');
const path = require('path');
const folder = process.argv[1];
(async () => {
    const editor = new NativeEditor(JSON.stringify({kind:'demo'}));
    try {
        await editor.save(path.join(folder, 'demo.comp'));
        await editor.save(path.join(folder, 'demo.picsie'));
        await editor.exportImage(path.join(folder, 'demo.png'), false);
    } finally { editor.close(); }
    const raster = await openEditor(path.join(folder, 'demo.comp'));
    try { await raster.save(path.join(folder, 'demo-raster.picsie')); }
    finally { raster.close(); }
})().catch(error => { console.error(error); process.exitCode = 1; });
""", str(folder)], cwd=Path(__file__).resolve().parent.parent, check=True)
    comp = folder/'demo.comp'
    manifest = json.loads((comp/'manifest.json').read_text())
    image = ET.Element('image', {'w': str(manifest['width']),
                                'h': str(manifest['height']), 'name': 'Color studies'})
    stack = ET.SubElement(image, 'stack')
    with zipfile.ZipFile(folder/'demo.ora', 'w') as archive:
        archive.writestr('mimetype', 'image/openraster', compress_type=zipfile.ZIP_STORED)
        for i, layer in enumerate(reversed(manifest['layers'])):
            src = f'data/layer{i}.png'
            origin = layer['transform']['origin']
            ET.SubElement(stack, 'layer', {
                'name': layer['name'], 'src': src,
                'x': str(round(origin['x'])), 'y': str(round(origin['y'])),
                'opacity': '1.0', 'visibility': 'visible',
                'composite-op': 'svg:screen' if layer['blendMode'] == 'Screen' else 'svg:src-over',
                'selected': 'true' if layer['name'] == 'Electric blue' else 'false'})
            archive.write(comp/'images'/layer['imageFile'], src)
        archive.writestr('stack.xml', ET.tostring(image))
        archive.write(folder/'demo.png', 'mergedimage.png')


def screenshot(window, path, env):
    subprocess.run(["import", "-window", window, str(path)], env=env, check=True)


def xdo(env, *arguments):
    return subprocess.check_output(["xdotool", *arguments], env=env, text=True).strip()


def drag(screen, app, excursion, initial, folder, env, window):
    # 120 inputs/sec. The faster case avoids the ~60 positions/sec ceiling of
    # the original 120 px triangle, whose movement is only 60 screen pixels/sec.
    screen.move(750, 610)
    screen.button(True)
    before = desktop.resources(app.pid)
    started = time.perf_counter()
    next_poll = 0.
    previous = screen.edge()
    changes, inputs, polls = [], [], []
    for step in range(1, 481):
        target = started + step / 120
        while time.perf_counter() < target:
            now = time.perf_counter() - started
            if now >= next_poll:
                edge = screen.edge()
                polls.append(now)
                if edge is not None and edge != previous:
                    changes.append([now, edge])
                    previous = edge
                next_poll = now + .004
            time.sleep(.0005)
        offset = (step if step <= 240 else 480-step) * excursion / 240
        screen.move(750 + offset, 610)
        inputs.append(time.perf_counter()-started)
    screen.button(False)
    duration = time.perf_counter()-started
    after = desktop.resources(app.pid)
    catchup = time.perf_counter()
    while abs(screen.edge()-initial) > 1:
        if time.perf_counter()-catchup > 10:
            raise RuntimeError("Drag did not return to its initial visible position")
        time.sleep(.004)
    catchup_ms = (time.perf_counter()-catchup)*1000
    assert len(changes) > 5
    assert max(p for _, p in changes)-initial >= excursion-10, changes
    intervals = [(b[0]-a[0])*1000 for a, b in zip(changes, changes[1:])]
    time.sleep(.5)
    screenshot(window, folder/f"after-drag-{excursion}.png", env)
    return {"excursion_screen_px": excursion, "input_rate_requested_hz": 120,
            "duration_s": duration, "visible_updates_per_second": len(changes)/duration,
            "visible_interval_ms": desktop.stats(intervals),
            "cpu_percent_one_core": (after['cpu_seconds']-before['cpu_seconds'])/duration*100,
            "end_catchup_ms": catchup_ms, "visible_changes": changes,
            "input_times_s": inputs, "poll_times_s": polls}


def run_one(name, trial, args, base_env, out):
    folder = out/f"{name}-{trial}"
    folder.mkdir(parents=True, exist_ok=True)
    env = dict(base_env)
    if name == "gimp":
        env.update(runtime_environment(args.runtime))
        env['LD_LIBRARY_PATH'] += os.pathsep + base_env['LD_LIBRARY_PATH']
        env['PATH'] = str(args.runtime/'usr/bin') + os.pathsep + env['PATH']
        config = Path(env['GIMP3_DIRECTORY'])
        (config/'gimprc').write_text(
            '(show-welcome-dialog no)\n(config-version "3.2.6")\n'
            '(check-updates no)\n(initial-zoom-to-fit yes)\n'
            '(devices-share-tool yes)\n')
        session = (args.runtime/'etc/gimp/3.0/sessionrc').read_text()
        session = session.replace('(size 800 600)', '(size 1280 860)')
        (config/'sessionrc.performance').write_text(session)
        command = [str(args.runtime/'usr/bin/gimp'), '--no-splash', '--new-instance',
                   '--console-messages', '--session=performance', str(args.fixture/'demo.ora')]
    else:
        command = [str(args.picsie)]
        if name == 'picsie-raster':
            command += ['--open', str(args.fixture/'demo-raster.picsie')]
    app = server = screen = None
    host = {'start': host_resources()}
    try:
        server = subprocess.Popen(['Xvfb', args.display, '-screen', '0', '1280x860x24',
                                   '-nolisten', 'tcp'], env=base_env,
                                  stdout=open(folder/'xvfb.log', 'w'), stderr=subprocess.STDOUT,
                                  start_new_session=True)
        time.sleep(.5)
        if server.poll() is not None:
            raise RuntimeError('Test display unavailable')
        screen = desktop.Screen(args.display)
        app = subprocess.Popen(command, env=env, stdout=open(folder/'app.log', 'w'),
                               stderr=subprocess.STDOUT, start_new_session=True)
        started = time.perf_counter()
        while True:
            matches = subprocess.run(
                ['xdotool', 'search', '--onlyvisible', '--name',
                 'GIMP' if name == 'gimp' else 'Picsie'],
                env=env, text=True, capture_output=True).stdout.splitlines()
            if matches and screen.edge() is not None:
                window = matches[-1]
                break
            if app.poll() is not None or time.perf_counter()-started > 120:
                raise RuntimeError(f'{name}: startup failed; see {folder}/app.log')
            time.sleep(.1)
        xdo(env, 'windowfocus', window)
        time.sleep(1)
        if name == 'gimp':
            xdo(env, 'key', 'Escape')
            xdo(env, 'mousemove', '18', '74', 'click', '1')
            time.sleep(.3)
            screenshot(window, folder/'setup-move.png', env)
            # Move selected layer; avoid picking another layer during the drag.
            xdo(env, 'mousemove', '28', '401', 'click', '1')
            time.sleep(.3)
            screenshot(window, folder/'setup-selected-layer.png', env)
            xdo(env, 'mousemove', '405', '841', 'click', '1', 'key', 'ctrl+a')
            xdo(env, 'type', '71.333333%')
            xdo(env, 'key', 'Return')
            time.sleep(.3)
            screenshot(window, folder/'setup-zoom.png', env)
            xdo(env, 'mousemove', '770', '610', 'click', '1', 'key', 'Tab')
            time.sleep(.3)
            # GIMP's rulers/scrollbars/status occupy 34 x 100 px; the resulting
            # visible canvas is 936 x 734, matching Picsie's canvas.
            xdo(env, 'windowsize', window, '970', '834')
        else:
            screen.move(750, 610)
            screen.button(True)
            screen.button(False)
        time.sleep(3)
        geometry = xdo(env, 'getwindowgeometry', '--shell', window)
        expected_size = (970, 834) if name == 'gimp' else (1280, 860)
        fields = dict(line.split('=', 1) for line in geometry.splitlines())
        assert (int(fields['WIDTH']), int(fields['HEIGHT'])) == expected_size, geometry
        screenshot(window, folder/'startup.png', env)
        initial = screen.edge()
        assert initial is not None
        before = desktop.resources(app.pid)
        idle_start = time.perf_counter()
        time.sleep(3)
        idle = desktop.resources(app.pid)
        idle_cpu = (idle['cpu_seconds']-before['cpu_seconds'])/(time.perf_counter()-idle_start)*100
        host['before_nudges'] = host_resources()
        latencies, displacements, polls, delays = [], [], [], []
        phase_random = random.Random(7391 + trial)
        for i in range(28):
            previous = screen.edge()
            started = time.perf_counter()
            screen.nudge('Right' if i % 2 == 0 else 'Left')
            count = 0
            while True:
                edge = screen.edge()
                count += 1
                if edge is not None and abs(edge-previous) >= 5:
                    break
                if time.perf_counter()-started > 5:
                    raise RuntimeError(f'{name}: nudge did not become visible')
                time.sleep(.004)
            duration = (time.perf_counter()-started)*1000
            expected = (24, 26) if name == 'gimp' else (6, 9)
            assert expected[0] <= abs(edge-previous) <= expected[1], (name, previous, edge)
            if i >= 4:
                latencies.append(duration)
                displacements.append(edge-previous)
                polls.append(count)
            delay = phase_random.uniform(.080, .147) if getattr(args, 'jitter_nudges', False) else .1
            delays.append(delay)
            time.sleep(delay)
        assert abs(screen.edge()-initial) <= 1
        host['after_nudges'] = host_resources()
        time.sleep(.5)
        drags = [drag(screen, app, n, initial, folder, env, window) for n in [120, 240]]
        settled = desktop.resources(app.pid)
        host['end'] = host_resources()
        result = {'app': name, 'trial': trial, 'command': command, 'window_geometry': geometry,
                  'canvas': [936, 734], 'zoom': .7133333333333334,
                  'initial_edge_x': initial, 'idle': idle,
                  'idle_cpu_percent_one_core': idle_cpu,
                  'nudge_latency_ms': desktop.stats(latencies), 'nudge_samples_ms': latencies,
                  'nudge_displacements_px': displacements, 'nudge_poll_counts': polls,
                  'nudge_delays_s': delays,
                  'drags': drags, 'settled': settled, 'host': host}
        (folder/'result.json').write_text(json.dumps(result, indent=2)+'\n')
        print(f'{name} {trial}: nudge={statistics.median(latencies):.1f}ms '
              f'p95={result["nudge_latency_ms"]["p95"]:.1f}ms '
              f'drag120={drags[0]["visible_updates_per_second"]:.1f}/s '
              f'drag240={drags[1]["visible_updates_per_second"]:.1f}/s', flush=True)
        return result
    finally:
        host['cleanup'] = host_resources()
        (folder/'host-resources.json').write_text(json.dumps(host, indent=2)+'\n')
        desktop.stop(app)
        if screen:
            screen.close()
        desktop.stop(server)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--runtime', type=Path, default=Path('artifacts/gimp-performance/runtime'))
    parser.add_argument('--fixture', type=Path, default=Path('artifacts/gimp-performance/fixture'))
    parser.add_argument('--picsie', type=Path, default=Path('crates/picsie-desktop/target/release/picsie-desktop'))
    parser.add_argument('--tools', type=Path, default=Path('artifacts/selection-history/tools/usr'))
    parser.add_argument('--output', type=Path, default=Path('artifacts/gimp-performance/comparison'))
    parser.add_argument('--display', default=':95')
    parser.add_argument('--trials', type=int, default=3)
    parser.add_argument('--jitter-nudges', action='store_true',
                        help='Use reproducible varied idle gaps to sample more refresh phases')
    parser.add_argument('--prepare-fixture', action='store_true')
    parser.add_argument('--apps', nargs='+', choices=['gimp', 'picsie', 'picsie-raster'],
                        default=['gimp', 'picsie', 'picsie-raster'])
    args = parser.parse_args()
    for name in ['runtime', 'fixture', 'picsie', 'tools', 'output']:
        setattr(args, name, getattr(args, name).resolve())
    if args.trials < 1:
        parser.error('--trials must be at least 1')
    if len(set(args.apps)) != len(args.apps):
        parser.error('--apps must be distinct')
    if args.prepare_fixture:
        prepare_fixture(args.fixture)
    args.output.mkdir(parents=True, exist_ok=True)
    env = dict(os.environ, DISPLAY=args.display, WINIT_UNIX_BACKEND='x11',
               XDG_CACHE_HOME=str(args.output/'cache'),
               PATH=str(args.tools/'bin')+os.pathsep+os.environ.get('PATH', ''),
               LD_LIBRARY_PATH=str(args.tools/'lib'),
               VK_DRIVER_FILES=str(args.tools/'share/vulkan/icd.d/lvp_icd.json'))
    for key in ['WAYLAND_DISPLAY', 'PICSIE_TRACE_DIR']:
        env.pop(key, None)
    version_env = {**env, **runtime_environment(args.runtime)}
    gimp_version = subprocess.check_output(
        [str(args.runtime/'usr/bin/gimp'), '--version'], env=version_env, text=True).strip()
    if not gimp_version.endswith('version 3.2.6'):
        parser.error(f'This pinned comparison requires GIMP 3.2.6; found {gimp_version}')
    results = []
    for trial in range(args.trials+1):
        order = args.apps[trial % len(args.apps):] + args.apps[:trial % len(args.apps)]
        for name in order:
            result = run_one(name, trial, args, env, args.output)
            if trial:
                results.append(result)
    summary = {}
    for name in args.apps:
        runs = [r for r in results if r['app'] == name]
        summary[name] = {'nudge_latency_ms': desktop.stats([v for r in runs for v in r['nudge_samples_ms']]),
                         'idle_rss_mib': desktop.stats([r['idle']['rss_mib'] for r in runs]),
                         'drags': {str(n): {
                             'updates_per_second': desktop.stats([r['drags'][i]['visible_updates_per_second'] for r in runs]),
                             'interval_ms': desktop.stats([v for r in runs for v in [
                                 (b[0]-a[0])*1000 for a,b in zip(r['drags'][i]['visible_changes'],r['drags'][i]['visible_changes'][1:])]])
                         } for i,n in enumerate([120,240])}}
    report = {'scope': 'External input to Xvfb framebuffer; no physical GPU or scanout',
              'document': 'Six layers, 1200 x 800; GIMP and picsie-raster use rasterized layer assets',
              'poll_interval_requested_ms': 4, 'trials': args.trials,
              'warmup': 'One complete discarded run per app; rotated measured launch order',
              'gimp_version': gimp_version,
              'binary_sha256': {name: hashlib.sha256(path.read_bytes()).hexdigest() for name,path in [
                  ('gimp',args.runtime/'usr/bin/gimp'),('picsie',args.picsie)]},
              'summary': summary, 'runs': results}
    (args.output/'application-results.json').write_text(json.dumps(report, indent=2)+'\n')
    print(json.dumps(summary, indent=2))


if __name__ == '__main__':
    main()
