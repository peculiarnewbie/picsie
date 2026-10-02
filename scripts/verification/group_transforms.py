"""Focused native verification for the group-transform box (transform.group-box).

Drives the real GPUI window on X11 through xdotool: folder + multi-layer box,
numeric resize/rotate/Apply as one undo, Escape cancel, handle resize/rotate
drag, Shift ratio, Flip H, locked-member exclusion, and a .comp save/reopen
check. Screenshots, trace and report stay in artifacts/group-transforms/native.
"""
import argparse
from environment import add_environment_arguments, native_environment, backend_description
import json
import os
import re
import shutil
import signal
import subprocess
import time
from pathlib import Path

parser = argparse.ArgumentParser()
parser.add_argument('--binary', default='crates/picsie-desktop/target/release/picsie-desktop')
parser.add_argument('--output', default='artifacts/group-transforms/native')
add_environment_arguments(parser)
parser.add_argument('--display', default=':111')
args = parser.parse_args()
out = Path(args.output).resolve()
out.mkdir(parents=True, exist_ok=True)
trace = out / 'trace'
trace.mkdir(exist_ok=True)
for file in trace.glob('window-*.json'):
    file.unlink()
config = out / 'config/xdg-desktop-portal'
config.mkdir(parents=True, exist_ok=True)
preferences = out / 'config/picsie/ui.json'
if preferences.exists():
    preferences.unlink()
(config / 'portals.conf').write_text('[preferred]\ndefault=gtk\n')
env = native_environment(args, out, trace)
processes = []
checks = []
window_number = 1
window_id = None

def x(*arguments):
    return subprocess.check_output(['xdotool', *map(str, arguments)], env=env, text=True, timeout=15).strip()

def state():
    try:
        return json.loads((trace / f'window-{window_number}.json').read_text())
    except (FileNotFoundError, json.JSONDecodeError):
        return {}

def wait(predicate, label='state transition', timeout=25):
    end = time.monotonic() + timeout
    while time.monotonic() < end:
        current = state()
        if current and predicate(current):
            return current
        time.sleep(.05)
    raise AssertionError(f'{label}: {json.dumps(state())[:2500]}')

def settle():
    # busy covers file/clipboard UI, including a chooser that needs driver input.
    # Engine rendering completes through the sequence barrier, not this flag.
    wait(lambda s: s['busy'] or s['sequence'] >= s.get('submittedSequence', 0), 'preview catches up')

def window_exists(id):
    return subprocess.run(['xdotool', 'getwindowname', id], env=env,
                          stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL).returncode == 0

def check(label, assertion):
    if callable(assertion):
        wait(lambda _: assertion(), label)
    else:
        assert assertion, label
    checks.append(label)
    print('PASS', label, flush=True)

def focus(title):
    global window_id
    end = time.monotonic() + 20
    while True:
        try:
            window_id = x('search', '--onlyvisible', '--name', title).splitlines()[-1]
            break
        except (subprocess.CalledProcessError, IndexError):
            if time.monotonic() >= end:
                raise
            time.sleep(.05)
    x('windowmove', window_id, 0, 0, 'windowraise', window_id, 'windowfocus', window_id)
    time.sleep(.25)

MENUS = {'new-canvas': 'File', 'save-comp': 'File', 'group': 'Layer'}

def prepare_control(control):
    if control in MENUS and control not in state()['controls']:
        click('menu-' + MENUS[control], scroll=False)
        return
    mask_controls = {'mask-add', 'mask-link', 'mask-enabled', 'mask-remove', 'mask-reset-reveal',
                     'mask-reset-hide', 'paint-content', 'paint-mask', 'clipping', 'select-mask-source'}
    adjustment_controls = {'field-brightness', 'field-saturation', 'field-blur', 'slider-brightness',
                           'slider-saturation', 'slider-blur'}
    properties_controls = {'field-name', 'lock', 'use-foreground', 'gradient-end'}
    palette = ('properties-menu' if control in mask_controls else 'adjustments-menu'
               if control in adjustment_controls else 'properties-menu'
               if control in properties_controls else None)
    current = state()
    if current.get('textPalette') and control != 'text-editor':
        key('Escape')
    if current.get('palette') and current['palette'] != palette and control != current['palette']:
        key('Escape')
    if palette and state().get('palette') != palette:
        click(palette, scroll=False)

def bounds(control, scroll=True):
    prepare_control(control)
    rect = None
    for _ in range(70):
        current = state()
        rect = current['controls'].get(control)
        if rect:
            left, top, width, height = rect
            canvas = current['controls']['canvas']
            bottom = canvas[1] + canvas[3]
            header_control = control in {'field-x', 'field-y', 'field-width', 'field-height',
                                         'field-rotation', 'field-transform-scale',
                                         'transform-ratio', 'select-transform-sampling',
                                         'flip-x', 'flip-y'}
            if scroll and not current['modal'] and header_control and current['state']['tool'] == 'move':
                clip = current['controls'].get('transform-fields', [112, top, 1048, height])
                start = clip[0]
                edge = start + clip[2]
                if left < start or left + width > edge:
                    x('mousemove', round((start + edge) / 2), round(top + height / 2),
                      'click', 7 if left + width > edge else 6)
                    time.sleep(.08)
                    continue
            if not scroll or current['modal'] or (not control.startswith('tool-')) or (84 <= top and top + height <= bottom):
                return rect
            x('mousemove', 28, round(canvas[1] + canvas[3] / 2), 'click', 5 if top + height > bottom else 4)
        time.sleep(.1)
    raise AssertionError(f'Cannot reveal {control}: {rect}')

def click(control, modifier=None, scroll=True):
    left, top, width, height = bounds(control, scroll)
    if modifier:
        x('keydown', *modifier.split())
    x('mousemove', round(left + width / 2), round(top + height / 2), 'click', 1)
    if modifier:
        x('keyup', *modifier.split())
    time.sleep(.15)
    settle()

def key(keys):
    x('key', '--clearmodifiers', keys)
    time.sleep(.16)
    settle()

def field(name, value, enter=True):
    click('field-' + name)
    key('ctrl+a')
    if str(value):
        x('type', '--clearmodifiers', '--', str(value))
    else:
        x('key', '--clearmodifiers', 'BackSpace')
    if enter:
        key('Return')
    time.sleep(.1)

def drag(x0, y0, x1, y1, modifier=None):
    if modifier:
        x('keydown', *modifier.split())
    x('mousemove', round(x0), round(y0), 'mousedown', 1)
    for step in range(1, 21):
        x('mousemove', round(x0 + (x1 - x0) * step / 20), round(y0 + (y1 - y0) * step / 20))
        time.sleep(.015)
    x('mouseup', 1)
    if modifier:
        x('keyup', *modifier.split())
    time.sleep(.25)
    settle()

def pt(xp, yp):
    s = state()
    v = s['state']['viewport']
    d = s['state']['document']
    r = s['controls']['canvas']
    z = v['zoom']
    return (r[0] + (r[2] - d['width'] * z) / 2 + v['pan']['x'] + xp * z,
            r[1] + (r[3] - d['height'] * z) / 2 + v['pan']['y'] + yp * z)

def shot(name):
    time.sleep(.3)
    subprocess.run(['import', '-window', window_id or 'root', str(out / name)],
                   env=env, check=True, timeout=15)

def layers():
    return state()['state']['document']['layers']

def selected():
    selected_id = state()['state']['selection']['ids'][-1]
    return next(layer for layer in layers() if layer['id'] == selected_id)

def group_box():
    return state()['state']['groupBox']

def portal(path):
    end = time.monotonic() + 55
    dialog = None
    while time.monotonic() < end:
        try:
            dialog = x('search', '--onlyvisible', '--class', 'Xdg-desktop-portal-gtk').splitlines()[-1]
            break
        except (subprocess.CalledProcessError, IndexError):
            time.sleep(.2)
    if not dialog:
        raise AssertionError('GTK portal file chooser did not open')
    x('windowfocus', dialog)
    key('ctrl+l')
    x('type', '--clearmodifiers', str(path))
    key('Return')
    time.sleep(.4)
    try:
        visible = x('search', '--onlyvisible', '--class', 'Xdg-desktop-portal-gtk')
        if dialog in visible.splitlines():
            key('Return')
    except subprocess.CalledProcessError:
        pass
    wait(lambda s: not s['busy'], 'file operation', timeout=30)
    focus(re.escape(state()['state']['document']['name']) + '.*Picsie')

def close(a, b):
    return abs(a - b) < .6

try:
    server = subprocess.Popen(['Xvfb', args.display, '-screen', '0', '1280x900x24', '-nolisten', 'tcp'],
                              env=env, stdout=open(out / 'xvfb.log', 'w'),
                              stderr=subprocess.STDOUT, start_new_session=True)
    processes.append(server)
    time.sleep(.5)
    app = subprocess.Popen(['dbus-run-session', '--', str(Path(args.binary).resolve())], env=env,
                           stdout=open(out / 'app.log', 'w'), stderr=subprocess.STDOUT,
                           start_new_session=True)
    processes.append(app)
    wait(lambda s: s.get('state') is not None, 'startup')
    focus('Color studies.*Picsie')

    # Fresh canvas for the group pass.
    click('new-canvas')
    field('new-name', 'Group transforms', False)
    field('new-width', 400, False)
    field('new-height', 300, False)
    click('modal-apply')
    window_number = 2
    wait(lambda s: s.get('state') is not None)
    focus('Group transforms.*Picsie')
    x('windowsize', window_id, 1281, 860)
    time.sleep(.3)
    click('fit')

    # Two shape members (vector content transforms by placement, like upstream).
    click('tool-rectangle')
    drag(*pt(40, 50), *pt(140, 110))
    red = selected()['id']
    drag(*pt(200, 120), *pt(300, 210))
    blue = selected()['id']
    click('tool-move')
    assert selected()['id'] == blue

    # Multi-select through the layer list: Ctrl-click toggles membership.
    click('layer-' + red, modifier='Control_L')
    check('multi selection publishes one shared group box',
          lambda: (sorted(state()['state']['selection']['ids']) == sorted([red, blue])
                   and group_box() is not None
                   and close(group_box()['x'], 40) and close(group_box()['y'], 50)
                   and close(group_box()['width'], 260) and close(group_box()['height'], 160)))
    target = state()['state']['transformTarget']
    check('numeric fields target the shared box, not one layer',
          lambda: (close(target['x'], 40) and close(target['width'] * target['scaleX'], 260)))
    shot('01-multi-box.png')

    # Numeric resize applies to both members as one undo.
    count = state()['state']['history']['undoCount']
    field('width', 520)
    check('numeric width opens a group draft', lambda: state()['state']['transformActive'])
    shot('02-width-draft.png')
    click('transform-apply')
    both = {layer['id']: layer for layer in layers()}
    check('numeric resize commits both members as one undo',
          lambda: (state()['state']['history']['undoCount'] == count + 1
                   and state()['state']['history']['undoLabel'] == 'Transform Layers'
                   and close(both[red]['width'] * both[red]['scaleX'], 200)
                   and close(both[blue]['x'], 360)))
    shot('03-resized.png')
    key('ctrl+z')

    # Escape cancels a numeric draft and restores both members.
    before = {layer['id']: (layer['x'], layer['y']) for layer in layers()}
    field('x', 999)
    check('numeric x opens a group draft', lambda: state()['state']['transformActive'])
    key('Escape')
    restored = {layer['id']: layer for layer in layers()}
    check('Escape cancels the group draft without history',
          lambda: (not state()['state']['transformActive']
                   and state()['state']['history']['undoCount'] == count
                   and all(close(restored[id]['x'], before[id][0]) for id in before)))
    shot('04-escaped.png')

    # Corner handle drag resizes the shared box as one gesture and one undo.
    box = group_box()
    start = (box['x'] + box['width'], box['y'] + box['height'])
    drag(*pt(*start), *pt(start[0] + 60, start[1] + 40))
    check('corner drag resizes the shared box as one undo',
          lambda: (state()['state']['history']['undoCount'] == count + 1
                   and close(group_box()['width'], box['width'] + 60)
                   and close(group_box()['height'], box['height'] + 40)))
    shot('05-handle-resize.png')
    key('ctrl+z')

    # Shift keeps box proportions on a handle drag.
    box = group_box()
    ratio = box['width'] / box['height']
    start = (box['x'] + box['width'], box['y'] + box['height'])
    drag(*pt(*start), *pt(start[0] + 80, start[1] + 10), modifier='Shift_L')
    check('Shift drag preserves box proportions',
          lambda: (close(group_box()['width'] / group_box()['height'], ratio)
                   and group_box()['width'] > box['width']))
    shot('06-shift-ratio.png')
    key('ctrl+z')

    # Rotation stalk drag rotates about the box center; Shift snaps to 15 degrees.
    box = group_box()
    zoom = state()['state']['viewport']['zoom']
    cx, cy = box['x'] + box['width'] / 2, box['y'] + box['height'] / 2
    stalk = (cx, box['y'] - 28 / zoom)
    before_ids = sorted(state()['state']['selection']['ids'])
    drag(*pt(*stalk), *pt(cx - 120, cy), modifier='Shift_L')
    rots = [layer['rotation'] for layer in layers() if layer['id'] in before_ids]
    check('stalk drag rotates the shared box with Shift snap',
          lambda: (sorted(state()['state']['selection']['ids']) == before_ids
                   and state()['state']['history']['undoCount'] == count + 1
                   and len(rots) == 2
                   and all(abs(r) > 1 and abs(r % 15) < .5 for r in rots)))
    shot('07-rotate.png')
    key('ctrl+z')

    # Flip H mirrors both members about the box middle as one undo.
    box = group_box()
    axis = box['x'] + box['width'] / 2
    centers = {layer['id']: layer['x'] + layer['width'] * layer['scaleX'] / 2 for layer in layers()}
    click('flip-x')
    after = {layer['id']: layer for layer in layers()}
    check('Flip H mirrors members about the box middle in one undo',
          lambda: (state()['state']['history']['undoCount'] == count + 1
                   and state()['state']['history']['undoLabel'] == 'Flip Horizontal'
                   and all(close(after[id]['x'] + after[id]['width'] * after[id]['scaleX'] / 2,
                                 2 * axis - centers[id]) for id in centers)))
    shot('08-flipped.png')
    key('ctrl+z')

    # A locked member stays out: the box shrinks to the unlocked member and the
    # numeric edit does not move the locked layer.
    click('layer-' + red)
    click('lock')
    check('lock control pins the layer', lambda: selected()['locked'])
    click('layer-' + blue, modifier='Control_L')
    check('locked member leaves the shared box',
          lambda: (sorted(state()['state']['selection']['ids']) == sorted([red, blue])
                   and group_box() is not None
                   and close(group_box()['width'], 100) and close(group_box()['height'], 90)))
    field('width', 200)
    click('transform-apply')
    locked = next(layer for layer in layers() if layer['id'] == red)
    check('group edit does not move the locked member',
          lambda: (close(locked['x'], 40) and close(locked['width'] * locked['scaleX'], 100)))
    shot('09-locked-excluded.png')
    key('ctrl+z')
    click('layer-' + red)
    click('lock')

    # Grouping into a folder keeps one box over the folder contents.
    click('layer-' + blue, modifier='Control_L')
    click('group')
    folder = selected()['id']
    check('folder selection keeps the shared box',
          lambda: (selected()['content']['kind'] == 'group' and group_box() is not None))
    count = state()['state']['history']['undoCount']
    field('rotation', 10)
    click('transform-apply')
    check('folder numeric rotation carries children as one undo',
          lambda: (state()['state']['history']['undoCount'] == count + 1
                   and all(abs(layer['rotation'] - 10) < .5 for layer in layers()
                           if layer['id'] in (red, blue))))
    shot('10-folder-rotate.png')

    # Save the transformed folder; the package keeps structure and placement.
    # (The portal may append its own extension; accept either spelling.)
    click('save-comp')
    portal(out / 'group.comp')
    candidates = [out / 'group.comp', out / 'group.comp.comp']
    package = next(p for p in candidates if (p / 'manifest.json').exists())
    manifest = json.loads((package / 'manifest.json').read_text())
    check('package keeps folder structure and carried placements',
          lambda: (any(layer.get('isGroup') for layer in manifest['layers'])
                   and sum(1 for layer in manifest['layers'] if not layer.get('isGroup')) == 2))
    shot('11-saved.png')

    (out / 'report.json').write_text(json.dumps(
        {'checks': checks, 'count': len(checks), 'backend': backend_description(env),
         'binary': args.binary,
         'source': '609dbeae2ef68ef4fc82d67e4981a49852eb6e13'}, indent=2) + '\n')
    (out / 'final-state.json').write_text(json.dumps(state(), indent=2) + '\n')
    print(f'All {len(checks)} group-transform checks passed. Screenshots: {out}', flush=True)
finally:
    for process in processes:
        try:
            if process is server:
                process.terminate()
            else:
                process.send_signal(signal.SIGTERM)
        except Exception:
            pass
