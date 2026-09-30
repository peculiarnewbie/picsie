"""Check X11 frame waking through real window visibility and focus changes.

Uses the same staged tools and edge detector as the performance comparison.
This is a local platform regression, not a translated upstream behavior fixture.
"""
import argparse
import ctypes as C
import importlib.util
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import time

sys.dont_write_bytecode = True
root = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location('desktop', root/'scripts/compare-desktop-performance.py')
desktop = importlib.util.module_from_spec(spec)
spec.loader.exec_module(desktop)
parser = argparse.ArgumentParser()
parser.add_argument('--binary', type=Path, default=root/'crates/picsie-desktop/target/release/picsie-desktop')
parser.add_argument('--tools', type=Path, default=root/'artifacts/selection-history/tools/usr')
parser.add_argument('--output', type=Path, default=root/'artifacts/frame-wakeup-verification')
parser.add_argument('--display', default=':94')
args = parser.parse_args()
out = args.output.resolve()
out.mkdir(parents=True, exist_ok=True)
env = {key: value for key, value in os.environ.items() if not key.startswith('PICSIE_')}
env.pop('WAYLAND_DISPLAY', None)
env.update(DISPLAY=args.display, WINIT_UNIX_BACKEND='x11',
           PATH=str(args.tools.resolve()/'bin')+os.pathsep+env.get('PATH', ''),
           LD_LIBRARY_PATH=str(args.tools.resolve()/'lib'),
           PICSIE_GPU_DIAGNOSTICS='1')
server = app = screen = None
checks = []


def xdo(*values):
    return subprocess.check_output(['xdotool', *map(str, values)], env=env,
                                   text=True, timeout=15).strip()


def wait(predicate, timeout=20):
    end = time.monotonic()+timeout
    while time.monotonic() < end:
        if app.poll() is not None:
            raise RuntimeError('Application exited during visibility/focus check')
        if predicate():
            return
        time.sleep(.02)
    raise TimeoutError('Window did not display the expected canvas')


def idle(label):
    before = desktop.resources(app.pid)
    start = time.perf_counter()
    time.sleep(2)
    after = desktop.resources(app.pid)
    cpu = (after['cpu_seconds']-before['cpu_seconds'])/(time.perf_counter()-start)*100
    assert cpu < 15, (label, cpu)
    checks.append({'check': label, 'cpu_percent_one_core': cpu})


try:
    server = subprocess.Popen(['Xvfb', args.display, '-screen', '0', '1280x860x24', '-nolisten', 'tcp'],
                              env=env, stdout=open(out/'xvfb.log', 'w'), stderr=subprocess.STDOUT,
                              start_new_session=True)
    time.sleep(.5)
    assert server.poll() is None, 'Display unavailable'
    screen = desktop.Screen(args.display)
    app = subprocess.Popen([str(args.binary.resolve())], env=env,
                           stdout=open(out/'app.log', 'w'), stderr=subprocess.STDOUT,
                           start_new_session=True)
    wait(lambda: screen.edge() is not None)
    window = xdo('search', '--onlyvisible', '--name', 'Picsie').splitlines()[-1]
    xdo('windowfocus', window)
    screen.move(750, 610)
    screen.button(True)
    screen.button(False)
    time.sleep(1)
    original = screen.edge()
    assert original is not None

    # Leave a redraw pending while hiding; remapping must still present fresh state.
    screen.nudge('Right')
    xdo('windowunmap', window)
    time.sleep(.3)
    idle('hidden window does not spin')
    xdo('windowmap', window, 'windowraise', window, 'windowfocus', window)
    wait(lambda: screen.edge() is not None and 6 <= screen.edge()-original <= 9)
    checks.append({'check': 'pending edit is visible after remapping'})

    # A small real X11 window takes focus without covering the tested canvas.
    for name, arguments, result in [
        ('XCreateSimpleWindow', [C.c_void_p, C.c_ulong, C.c_int, C.c_int, C.c_uint,
                                C.c_uint, C.c_uint, C.c_ulong, C.c_ulong], C.c_ulong),
        ('XMapWindow', [C.c_void_p, C.c_ulong], C.c_int),
        ('XSetInputFocus', [C.c_void_p, C.c_ulong, C.c_int, C.c_ulong], C.c_int),
        ('XDestroyWindow', [C.c_void_p, C.c_ulong], C.c_int),
    ]:
        function = getattr(screen.x, name)
        function.argtypes, function.restype = arguments, result
    auxiliary = screen.x.XCreateSimpleWindow(screen.display, screen.root, 1235, 5, 32, 32, 0, 0, 0)
    screen.x.XMapWindow(screen.display, auxiliary)
    screen.x.XSetInputFocus(screen.display, auxiliary, 2, 0)
    screen.flush()
    time.sleep(.3)
    idle('inactive window does not spin')
    xdo('windowfocus', window)
    screen.nudge('Left')
    wait(lambda: screen.edge() == original)
    checks.append({'check': 'input and canvas recover after focus returns'})
    screen.x.XDestroyWindow(screen.display, auxiliary)
    screen.flush()
    subprocess.run(['magick', 'import', '-window', window, str(out/'recovered.png')], env=env, check=True)
    (out/'report.json').write_text(json.dumps({'checks': checks, 'passed': len(checks)}, indent=2)+'\n')
    print(json.dumps(checks, indent=2), flush=True)
finally:
    if screen is not None:
        screen.close()
    for process in [app, server]:
        if process is not None:
            try:
                os.killpg(process.pid, signal.SIGTERM)
            except ProcessLookupError:
                pass
            try:
                process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                os.killpg(process.pid, signal.SIGKILL)
                process.wait()
