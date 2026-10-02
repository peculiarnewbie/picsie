"""Observe matched brush/eraser strokes in the native Picsie and GIMP windows.

Same one-layer fixtures as compare-gimp-behaviors.py. Real AMD rendering with
Xvfb presentation is supported; results exclude a physical compositor/scanout.
An untimed traced Picsie probe locates its controls. Measured apps are untraced.
"""
import argparse
import ctypes as C
import hashlib
import importlib.util
import json
import math
import os
from pathlib import Path
import subprocess
import sys
import time

sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parent.parent
spec = importlib.util.spec_from_file_location('movement_comparison', ROOT/'scripts/compare-gimp-performance.py')
comparison = importlib.util.module_from_spec(spec)
spec.loader.exec_module(comparison)


class Screen(comparison.desktop.Screen):
    def row(self, x, y, width):
        image = self.x.XGetImage(self.display, self.root, x, y, width, 1, C.c_ulong(-1).value, 2)
        if not image:
            raise RuntimeError('XGetImage failed')
        try:
            assert image.contents.bits_per_pixel == 32 and image.contents.byte_order == 0
            return C.string_at(image.contents.data, width*4)
        finally:
            self.x.XDestroyImage(image)

    def chord(self, key, modifier=None):
        if modifier:
            self.key(modifier, True)
        self.key(key, True)
        self.key(key, False)
        if modifier:
            self.key(modifier, False)
        self.flush()


def changed_bounds(data, x, tool):
    # Require a solid run, so the thin brush cursor outline cannot masquerade as paint.
    matched = []
    run = []
    for i in range(len(data)//4):
        b, g, r = data[i*4:i*4+3]
        valid = max(r, g, b) < 12 if tool == 'brush' else max(r, g, b)-min(r, g, b) < 4
        if valid:
            run.append(x+i)
        else:
            if len(run) >= 8:
                matched.extend(run)
            run = []
    if len(run) >= 8:
        matched.extend(run)
    return (min(matched), max(matched)) if matched else None


def trace_state(folder):
    try:
        return json.loads((folder/'trace/window-1.json').read_text())
    except (FileNotFoundError, json.JSONDecodeError):
        return {}


def read_until(callback, label, timeout=120):
    started = time.monotonic()
    while time.monotonic()-started < timeout:
        value = callback()
        if value:
            return value
        time.sleep(.01)
    raise RuntimeError(f'Timed out: {label}')


def launch(name, width, trial, args, base_env, size_field=None, probe=False):
    folder = args.output/('setup-probe' if probe else f'{name}-{width}-{trial}')
    folder.mkdir(parents=True)
    env = dict(base_env)
    if probe:
        (folder/'trace').mkdir()
        env['PICSIE_TRACE_DIR'] = str(folder/'trace')
    else:
        env.pop('PICSIE_TRACE_DIR', None)
    zoom = min(1., 856/width, 654/(width*2/3))
    server = app = screen = None
    try:
        server = subprocess.Popen(['Xvfb', args.display, '-screen', '0', '1280x860x24', '-nolisten', 'tcp'],
                                  env=base_env, stdout=(folder/'xvfb.log').open('w'),
                                  stderr=subprocess.STDOUT, start_new_session=True)
        time.sleep(.5)
        assert server.poll() is None, 'Test display unavailable'
        screen = Screen(args.display)
        if name == 'gimp':
            env.update(comparison.runtime_environment(args.runtime))
            config = folder/'gimp-config'
            config.mkdir()
            (config/'tool-options').mkdir()
            (config/'gimprc').write_text(
                '(show-welcome-dialog no)\n(check-updates no)\n(config-version "3.2.6")\n'
                '(initial-zoom-to-fit yes)\n(devices-share-tool yes)\n(use-opencl no)\n'
                '(global-brush no)\n(global-dynamics no)\n')
            session = (args.runtime/'etc/gimp/3.0/sessionrc').read_text().replace('(size 800 600)', '(size 1280 860)')
            (config/'sessionrc.performance').write_text(session)
            for tool in ['paintbrush', 'eraser']:
                (config/'tool-options'/f'gimp-{tool}-tool').write_text(
                    f'(tool "gimp-{tool}-tool")\n(brush "2. Hardness 100")\n'
                    '(brush-size 100)\n(brush-hardness 1)\n(brush-spacing 0.015)\n'
                    '(brush-force 0.5)\n(opacity 1)\n(dynamics-enabled no)\n'
                    '(use-smoothing no)\n')
            env['GIMP3_DIRECTORY'] = str(config)
            env['GIMP3_CACHEDIR'] = str(folder/'gimp-cache')
            env['PATH'] = str(args.runtime/'usr/bin')+os.pathsep+base_env['PATH']
            env['LD_LIBRARY_PATH'] += os.pathsep+base_env['LD_LIBRARY_PATH']
            command = [str(args.runtime/'usr/bin/gimp'), '--new-instance',
                       '--console-messages', '--session=performance', str(args.fixtures/f'{width}.png')]
        else:
            command = [str(args.picsie), '--open', str(args.fixtures/f'{width}.picsie')]
        app = subprocess.Popen(command, env=env, stdout=(folder/'app.log').open('w'),
                               stderr=subprocess.STDOUT, start_new_session=True)
        focused_windows = set()
        def find_window():
            assert app.poll() is None, f'Application exited; see {folder}/app.log'
            if name == 'gimp':
                # Focusing GIMP's startup splash also initializes its device manager.
                early = subprocess.run(['xdotool', 'search', '--onlyvisible', '--name', 'GIMP'],
                                       env=env, text=True, capture_output=True).stdout.splitlines()
                for candidate in early:
                    if candidate not in focused_windows:
                        comparison.xdo(env, 'windowfocus', candidate)
                        focused_windows.add(candidate)
            matches = subprocess.run(['xdotool', 'search', '--onlyvisible', '--name',
                                      f'\\[{width}\\].*GIMP' if name == 'gimp' else 'Picsie'], env=env,
                                     text=True, capture_output=True).stdout.splitlines()
            return matches[-1] if matches else None
        window = read_until(find_window, 'application window')
        comparison.xdo(env, 'windowfocus', window)
        time.sleep(2)
        if name == 'gimp':
            comparison.xdo(env, 'key', 'Escape', 'key', 'm')
            comparison.xdo(env, 'mousemove', '405', '841', 'click', '1', 'key', 'ctrl+a')
            comparison.xdo(env, 'type', f'{zoom*100:.8f}%')
            comparison.xdo(env, 'key', 'Return')
            time.sleep(.3)
            comparison.xdo(env, 'mousemove', '770', '610', 'click', '1', 'key', 'Tab')
            comparison.xdo(env, 'windowsize', window, '970', '834')
            time.sleep(1)
            # Window resizing can retain an off-center scroll position. Center
            # explicitly before calibration; Shift+J is GIMP's pinned action.
            screen.chord('J', 'Shift_L')
            time.sleep(.3)
            canvas = [20, 46, 936, 734]
            screen.chord('p')
            # GIMP 3.2 initializes input devices on the first canvas focus event.
            # Force a fresh focus transition after file loading, without a WM.
            comparison.xdo(env, 'windowfocus', str(screen.root))
            time.sleep(.15)
            comparison.xdo(env, 'windowfocus', window)
        else:
            comparison.xdo(env, 'windowsize', window, '1245', '848')
            time.sleep(1)
            comparison.xdo(env, 'mousemove', '1111', '20', 'click', '1')
            screen.chord('b')
            time.sleep(.5)
            if probe:
                state = read_until(lambda: trace_state(folder).get('controls', {}).get('field-brush-size'), 'brush size control')
                size_field = [round(state[0]+state[2]/2), round(state[1]+state[3]/2)]
            comparison.xdo(env, 'mousemove', *map(str, size_field), 'click', '1', 'key', 'ctrl+a')
            comparison.xdo(env, 'type', '100')
            comparison.xdo(env, 'key', 'Return')
            canvas = [56, 84, 936, 734]
            if probe:
                state = read_until(lambda: trace_state(folder) if trace_state(folder).get('state', {}).get('brushSize') == 100 else None,
                                   '100-pixel brush')
                (folder/'controls.json').write_text(json.dumps(state, indent=2))
                comparison.screenshot(window, folder/'setup.png', env)
                return size_field
        time.sleep(1)
        comparison.screenshot(window, folder/'startup.png', env)
        origin_x = canvas[0]+(936-width*zoom)/2
        start_x = round(origin_x+width*zoom/12)
        end_x = round(origin_x+width*zoom*7/12)
        center_y = round(canvas[1]+734/2)
        radius = 100*zoom/2
        dy = round(radius/2)
        row_y = center_y+dy
        row_x = round(origin_x)
        row_width = round(width*zoom)
        baseline = screen.row(canvas[0], center_y, canvas[2])
        blue = [canvas[0]+i for i in range(canvas[2])
                if abs(baseline[i*4]-255) <= 2 and abs(baseline[i*4+1]-135) <= 2
                and abs(baseline[i*4+2]-101) <= 2]
        # GTK borders and the marching-ant outline account for up to four
        # pixels in the nominal canvas bounds; large retained scroll offsets fail.
        assert blue and abs(blue[0]-origin_x) <= 4 and abs(blue[-1]-(origin_x+width*zoom-1)) <= 4, \
            f'Fixture is not centered at the expected fit zoom: {name} {width}'
        assert len(blue) >= row_width-5, 'Fixture baseline is not intact'
        analytic_tip = math.floor(math.sqrt(radius**2-dy**2)-.5)
        host = {'start': comparison.host_resources()}
        strokes = []
        for tool in ['brush', 'eraser']:
            screen.chord(('b' if name == 'picsie' else 'p') if tool == 'brush' else ('e' if name == 'picsie' else 'E'),
                         'Shift_L' if name == 'gimp' and tool == 'eraser' else None)
            # GIMP's eraser shortcut is Shift+E; native Picsie's is E.
            time.sleep(.4)
            def bounds():
                return changed_bounds(screen.row(row_x, row_y, row_width), row_x, tool)
            # Untimed single-dab calibration accounts for rasterized brush-edge
            # conventions without weakening the completed-stroke assertion.
            screen.move(start_x, center_y)
            time.sleep(.15)
            screen.button(True)
            screen.button(False)
            screen.move(980, 800)
            calibrated = read_until(bounds, 'brush edge calibration')
            time.sleep(.3)
            calibrated = bounds()
            assert calibrated and abs(calibrated[1]-start_x-analytic_tip) <= 5, (name, width, tool, calibrated, analytic_tip)
            expected_right = end_x + calibrated[1]-start_x
            screen.chord('z', 'Control_L')
            read_until(lambda: not bounds(), 'calibration undo', timeout=120)
            time.sleep(.4)
            for iteration in range(args.strokes+1):
                read_until(lambda: not bounds(), 'unpainted baseline', timeout=20)
                screen.move(start_x, center_y)
                time.sleep(.15)
                if iteration > 0:
                    # Preserve evidence that timing began, even if a later
                    # assertion fails before the screenshot/result is written.
                    (folder/'timed-input-started').touch()
                before = comparison.desktop.resources(app.pid)
                started = time.perf_counter()
                screen.button(True)
                changes, inputs, polls = [], [], []
                previous = None
                next_poll = 0.
                def poll():
                    nonlocal previous, next_poll
                    now = time.perf_counter()-started
                    if now < next_poll:
                        return None
                    found = bounds()
                    polls.append(now)
                    if found is not None and found[1] != previous:
                        changes.append([now, *found])
                        previous = found[1]
                    next_poll = now+.004
                    return found
                for step in range(1, 61):
                    target = started+step/60
                    while time.perf_counter() < target:
                        poll()
                        time.sleep(.0005)
                    screen.move(start_x+(end_x-start_x)*step/60, center_y)
                    inputs.append(time.perf_counter()-started)
                screen.button(False)
                input_end = time.perf_counter()-started
                screen.move(980, 800)
                while True:
                    found = poll()
                    if found and found[1] >= expected_right-2:
                        break
                    if time.perf_counter()-started > 120:
                        raise RuntimeError(f'Stroke endpoint not rendered: {name} {width} {tool}; last={previous}, expected={expected_right}')
                    time.sleep(.001)
                elapsed = time.perf_counter()-started
                after = comparison.desktop.resources(app.pid)
                final = bounds()
                assert final and final[1]-final[0] >= end_x-start_x-2, (tool, final)
                # A wrong restored brush size changes the outline extent and fails this check.
                assert abs(final[1]-expected_right) <= 3, (name, width, tool, final, expected_right)
                time.sleep(.15)
                comparison.screenshot(window, folder/f'{tool}-{iteration}.png', env)
                intervals = [(b[0]-a[0])*1000 for a, b in zip(changes, changes[1:])]
                result = dict(tool=tool, warmup=iteration == 0, input_duration_s=input_end,
                              endpoint_after_release_ms=max(0., elapsed-input_end)*1000,
                              first_visible_ms=changes[0][0]*1000,
                              updates_during_input=sum(t <= input_end for t, *_ in changes)/input_end,
                              updates_until_endpoint=len(changes)/elapsed,
                              calibrated_endpoint_x=expected_right, observed_endpoint_x=final[1],
                              interval_ms=comparison.desktop.stats(intervals) if intervals else None,
                              cpu_percent_one_core=(after['cpu_seconds']-before['cpu_seconds'])/elapsed*100,
                              visible_changes=changes, input_times_s=inputs, poll_times_s=polls)
                strokes.append(result)
                screen.chord('z', 'Control_L')
                read_until(lambda: not bounds(), 'undo restores source', timeout=120)
                time.sleep(.4)
            host[f'after_{tool}'] = comparison.host_resources()
        result = dict(app=name, width=width, height=width*2//3, trial=trial,
                      brush_diameter_doc_px=100, spacing_percent=1.5,
                      requested_input_hz=60, trajectory_doc_px=width/2,
                      trajectory_screen_px=end_x-start_x, zoom=zoom, canvas=canvas,
                      command=command, host=host, strokes=strokes)
        (folder/'result.json').write_text(json.dumps(result, indent=2)+'\n')
        return result
    except Exception:
        if app and screen:
            try:
                comparison.screenshot(window, folder/'failure.png', env)
            except Exception:
                pass
        raise
    finally:
        comparison.desktop.stop(app)
        if screen:
            screen.close()
        comparison.desktop.stop(server)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path, default=Path('artifacts/perf-behaviors-2026-10-01/strokes'))
    parser.add_argument('--fixtures', type=Path, default=Path('artifacts/perf-behaviors-2026-10-01/fixtures'))
    parser.add_argument('--runtime', type=Path, default=Path('artifacts/gimp-performance/runtime'))
    parser.add_argument('--picsie', type=Path, default=Path('crates/picsie-desktop/target/release/picsie-desktop'))
    parser.add_argument('--display', default=':97')
    parser.add_argument('--tools', type=Path, default=Path('/usr'))
    parser.add_argument('--gpu-icd', type=Path, help='Override the historical RADV driver explicitly')
    parser.add_argument('--trials', type=int, default=3)
    parser.add_argument('--strokes', type=int, default=2)
    parser.add_argument('--apps', nargs='+', choices=['picsie', 'gimp'], default=['picsie', 'gimp'])
    parser.add_argument('--widths', nargs='+', type=int, choices=[1200, 3600], default=[1200, 3600])
    parser.add_argument('--resume', action='store_true',
                        help='Resume a setup failure; preserve all completed launches and original manifest')
    args = parser.parse_args()
    if args.trials < 1 or args.strokes < 1:
        parser.error('Trials and strokes must be positive')
    for name in ['output', 'fixtures', 'runtime', 'picsie']:
        setattr(args, name, getattr(args, name).resolve())
    if args.output.exists() and not args.resume:
        parser.error('Choose a new output directory to preserve previous cohorts')
    args.output.mkdir(parents=True, exist_ok=args.resume)
    tools = args.tools.resolve()
    env = {**os.environ, 'DISPLAY': args.display, 'WINIT_UNIX_BACKEND': 'x11',
           'VK_DRIVER_FILES': str(args.gpu_icd.resolve() if args.gpu_icd else '/usr/share/vulkan/icd.d/radeon_icd.json'), 'MESA_VK_WSI_DEBUG': 'sw',
           'PICSIE_GPU_DIAGNOSTICS': '1', 'GEGL_USE_OPENCL': 'no',
           'XDG_CONFIG_HOME': str(args.output/'config'), 'XDG_CACHE_HOME': str(args.output/'cache'),
           'PATH': str(tools/'bin')+os.pathsep+os.environ.get('PATH', ''),
           'LD_LIBRARY_PATH': str(tools/'lib')}
    env.pop('WAYLAND_DISPLAY', None)
    manifest_path = args.output/'manifest.json'
    if args.resume:
        manifest = json.loads(manifest_path.read_text())
        assert manifest['trials'] == args.trials
        assert manifest['measured_strokes_per_tool_per_launch'] == args.strokes
        assert manifest.get('apps', ['picsie', 'gimp']) == args.apps
        assert manifest.get('widths', [1200, 3600]) == args.widths
        assert manifest['native_binary_sha256'] == hashlib.sha256(args.picsie.read_bytes()).hexdigest()
        size_field = manifest['native_size_field']
        stamp = str(time.time_ns())
        source_path = args.output/f'continuation-{stamp}.py'
        source_path.write_bytes(Path(__file__).read_bytes())
        note = dict(source_sha256=hashlib.sha256(source_path.read_bytes()).hexdigest(),
                    source_snapshot=source_path.name, setup_failures=[])
        manifest.setdefault('continuations', []).append(note)
    else:
        size_field = launch('picsie', 1200, -1, args, env, probe=True) if 'picsie' in args.apps else None
        manifest = dict(scope='External input to Xvfb framebuffer; physical compositor/scanout excluded',
                    trials=args.trials, measured_strokes_per_tool_per_launch=args.strokes,
                    apps=args.apps, widths=args.widths,
                    warmup='Untimed tip calibration; one entire discarded launch per app/size plus first stroke per tool in every launch',
                    native_size_field=size_field,
                    source_sha256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
                    native_binary_sha256=hashlib.sha256(args.picsie.read_bytes()).hexdigest())
    manifest_path.write_text(json.dumps(manifest, indent=2)+'\n')
    runs = []
    for trial in range(args.trials+1):
        for width in args.widths:
            names = args.apps if trial % 2 == 0 else list(reversed(args.apps))
            for app in names:
                folder = args.output/f'{app}-{width}-{trial}'
                if args.resume and (folder/'result.json').exists():
                    run = json.loads((folder/'result.json').read_text())
                    assert (run['app'], run['width'], run['trial']) == (app, width, trial)
                    assert len(run['strokes']) == 2*(args.strokes+1)
                    runs.append(run)
                    continue
                if args.resume and folder.exists():
                    # Never silently replace an incomplete timed launch. A setup
                    # failure may be retried only before measured screenshots exist.
                    assert not (folder/'timed-input-started').exists() and not any(
                        (folder/f'{tool}-{i}.png').exists()
                        for tool in ['brush', 'eraser'] for i in range(1, args.strokes+1)), \
                        f'Partially measured launch requires explicit review: {folder}'
                    failed = folder.with_name(folder.name+f'-setup-failed-{stamp}')
                    folder.rename(failed)
                    note['setup_failures'].append(failed.name)
                    manifest_path.write_text(json.dumps(manifest, indent=2)+'\n')
                run = launch(app, width, trial, args, env, size_field)
                runs.append(run)
                print(app, width, trial, [(s['tool'], round(s['first_visible_ms']),
                                          round(s['endpoint_after_release_ms'])) for s in run['strokes']], flush=True)
    summary = {}
    for app in args.apps:
        summary[app] = {}
        for width in args.widths:
            for tool in ['brush', 'eraser']:
                rows = [stroke for run in runs if run['app'] == app and run['width'] == width and run['trial'] > 0
                        for stroke in run['strokes'] if stroke['tool'] == tool and not stroke['warmup']]
                summary[app][f'{width}-{tool}'] = {metric: comparison.desktop.stats([row[metric] for row in rows])
                    for metric in ['first_visible_ms', 'endpoint_after_release_ms', 'updates_during_input',
                                   'updates_until_endpoint', 'cpu_percent_one_core']}
    (args.output/'results.json').write_text(json.dumps(dict(manifest=manifest, summary=summary, runs=runs), indent=2)+'\n')
    print(json.dumps(summary, indent=2), flush=True)


if __name__ == '__main__':
    main()
