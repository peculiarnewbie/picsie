"""Exercise the real GPUI window through X11 input and the real GTK file portal.

Requires Xvfb, xdotool, ImageMagick, dbus-run-session and xdg-desktop-portal-gtk.
--tools points to extracted bin/lib/share tools; --software selects their lavapipe.
All documents, traces and screenshots stay in the ignored output directory.
"""
import argparse
import json
import os
from pathlib import Path
import signal
import shutil
import subprocess
import time

parser = argparse.ArgumentParser()
parser.add_argument('--binary', default='crates/picsie-desktop/target/release/picsie-desktop')
parser.add_argument('--output', default='artifacts/gpui-parity/verification')
parser.add_argument('--tools', type=Path)
parser.add_argument('--software', action='store_true')
parser.add_argument('--display', default=':95')
args = parser.parse_args()
out = Path(args.output).resolve()
out.mkdir(parents=True, exist_ok=True)
trace = out / 'trace'
trace.mkdir(exist_ok=True)
for name in ['parity.picsie', 'parity-as.picsie', 'parity.png', 'parity.jpg', 'parity.comp', 'report.json', 'final-state.json', 'failure.png']:
    file = out / name
    if file.is_dir(): shutil.rmtree(file)
    elif file.exists(): file.unlink()
for file in trace.glob('window-*.json'):
    file.unlink()
config = out / 'config/xdg-desktop-portal'
config.mkdir(parents=True, exist_ok=True)
(config / 'portals.conf').write_text('[preferred]\ndefault=gtk\n')
env = {**os.environ, 'DISPLAY': args.display, 'PICSIE_TRACE_DIR': str(trace),
       'XDG_CONFIG_HOME': str(out / 'config'), 'XDG_DATA_HOME': str(out / 'data'), 'XDG_CURRENT_DESKTOP': 'GPUIParity'}
env.pop('WAYLAND_DISPLAY', None)
if args.tools:
    tools = args.tools.resolve()
    env['PATH'] = str(tools / 'bin') + os.pathsep + env.get('PATH', '')
    env['LD_LIBRARY_PATH'] = str(tools / 'lib') + os.pathsep + env.get('LD_LIBRARY_PATH', '')
    if args.software:
        env['VK_DRIVER_FILES'] = str(tools / 'share/vulkan/icd.d/lvp_icd.json')
if args.software and not args.tools:
    drivers = sorted(Path('/usr/share/vulkan/icd.d').glob('lvp_icd*.json'))
    if not drivers: raise SystemExit('Mesa lavapipe ICD not found; install mesa-vulkan-drivers')
    env['VK_DRIVER_FILES'] = str(drivers[0])
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

def wait(predicate, label='state transition', timeout=20):
    end = time.monotonic() + timeout
    while time.monotonic() < end:
        current = state()
        if current and predicate(current):
            return current
        time.sleep(.05)
    raise AssertionError(f'{label}: {json.dumps(state())[:2500]}')

def settle():
    wait(lambda s: s['busy'] or s['sequence'] >= s.get('submittedSequence', 0), 'preview catches up')

def window_exists(id):
    return subprocess.run(['xdotool','getwindowname',id],env=env,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL).returncode == 0

def check(label, assertion):
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
            if time.monotonic() >= end: raise
            time.sleep(.05)
    x('windowmove', window_id, 0, 0, 'windowraise', window_id, 'windowfocus', window_id)
    time.sleep(.25)

def bounds(control, scroll=True):
    for _ in range(70):
        rect = state()['controls'].get(control)
        if rect:
            left, top, width, height = rect
            if not scroll or state()['modal'] or (104 <= top and top + height <= 828) or left < 1000:
                return rect
            x('mousemove', 1274, 650, 'click', 5 if top + height > 828 else 4)
        else:
            raise AssertionError(f'Missing control: {control}')
        time.sleep(.2)
    raise AssertionError(f'Cannot scroll to {control}: {rect}')

def click(control, modifier=None, scroll=True):
    left, top, width, height = bounds(control, scroll)
    if modifier:
        x('keydown', modifier)
    x('mousemove', round(left + width / 2), round(top + height / 2), 'click', 1)
    if modifier:
        x('keyup', modifier)
    time.sleep(.15)
    settle()

def key(keys):
    x('key', '--clearmodifiers', keys)
    time.sleep(.16)
    settle()

def field(name, value, enter=True):
    click('field-' + name)
    key('ctrl+a')
    x('type', '--clearmodifiers', str(value))
    if enter:
        key('Return')
    time.sleep(.1)

def choose(control, index):
    values = {'font':['sans-serif','serif','monospace'], 'blend':['source-over','multiply','screen','overlay','darken','lighten','soft-light','hard-light','difference','exclusion','color-dodge','color-burn','hue','saturation','color','luminosity']}
    if control in values:
        value = selected()['content']['fontFamily'] if control=='font' else selected()['blend']
        current = values[control].index(value)
    elif control == 'mask-source':
        value = selected()['maskSourceId']
        current = state()['state']['maskSourceIds'].index(value) if value else -1
    else:
        current = 0 # Fresh canvas-size dialogs start at Pixels / Transparent.
    position = (selected()['x'], selected()['y']) if layers() else None
    click('select-'+control)
    for _ in range(abs(index-current)):key('Down' if index>current else 'Up')
    key('Return')
    if position is not None: assert (selected()['x'], selected()['y']) == position, 'Selector navigation must not nudge the layer'

def drag(x0, y0, x1, y1, modifier=None):
    if modifier:
        x('keydown', modifier)
    x('mousemove', round(x0), round(y0), 'mousedown', 1)
    for step in range(1, 21):
        x('mousemove', round(x0 + (x1-x0)*step/20), round(y0 + (y1-y0)*step/20))
        time.sleep(.015)
    x('mouseup', 1)
    if modifier:
        x('keyup', modifier)
    time.sleep(.25)
    settle()

def shot(name):
    time.sleep(.3)
    subprocess.run(['import', '-window', 'root', str(out / name)], env=env, check=True, timeout=15)

def layers(): return state()['state']['document']['layers']
def selected():
    selected_id = state()['state']['selection']['ids'][-1]
    return next(layer for layer in layers() if layer['id'] == selected_id)

def portal(path, save=True):
    end = time.monotonic() + 55
    dialog = None
    while time.monotonic() < end:
        try:
            dialog = x('search', '--onlyvisible', '--class', 'Xdg-desktop-portal-gtk').splitlines()[-1]
            break
        except (subprocess.CalledProcessError, IndexError):
            time.sleep(.2)
    if not dialog:
        raise AssertionError('GTK portal file chooser did not open: ' + state()['notice'])
    x('windowfocus', dialog)
    key('ctrl+a' if save else 'ctrl+l')
    x('type', '--clearmodifiers', str(path))
    key('Return')
    # GTK opens a parent directory before submitting a new absolute filename.
    time.sleep(.4)
    try:
        visible = x('search', '--onlyvisible', '--class', 'Xdg-desktop-portal-gtk')
        if dialog in visible.splitlines(): key('Return')
    except subprocess.CalledProcessError:
        pass
    wait(lambda s: not s['busy'], 'file operation', timeout=30)
    focus('GPUI parity.*Picsie')

try:
    server = subprocess.Popen(['Xvfb', args.display, '-screen', '0', '1280x900x24', '-nolisten', 'tcp'], env=env,
                              stdout=open(out/'xvfb.log', 'w'), stderr=subprocess.STDOUT, start_new_session=True)
    processes.append(server)
    time.sleep(.5)
    app = subprocess.Popen(['dbus-run-session', '--', str(Path(args.binary).resolve())], env=env,
                           stdout=open(out/'app.log', 'w'), stderr=subprocess.STDOUT, start_new_session=True)
    processes.append(app)
    wait(lambda s: s.get('state') is not None, 'startup')
    focus('Color studies.*Picsie')
    check('native window matches installed Picsie desktop identity', window_id in x('search', '--onlyvisible', '--class', '^picsie$').splitlines())
    check('936×734 native canvas and eleven tools', state()['controls']['canvas'] == [64.,98.,936.,734.] and len([k for k in state()['controls'] if k.startswith('tool-')]) == 11)
    shot('01-layout.png')
    click('new')
    field('new-name', 'GPUI parity', False)
    field('new-width', '400', False)
    field('new-height', '300', False)
    click('modal-apply')
    window_number = 2
    wait(lambda s: s.get('state') is not None, 'new window')
    focus('GPUI parity.*Picsie')
    check('new document opens a separate window', state()['state']['document']['width'] == 400 and state()['state']['document']['height'] == 300 and not layers())
    click('tool-rectangle')
    drag(350,335,620,500)
    wait(lambda s: len(s['state']['document']['layers']) == 1)
    check('rectangle gesture creates layer', selected()['content']['kind'] == 'shape')
    rectangle = selected()['id']
    count = state()['state']['history']['undoCount']
    click('tool-move')
    old_x = selected()['x']
    drag(450,410,480,425)
    check('move gesture is one undo', selected()['x'] > old_x and state()['state']['history']['undoCount'] == count+1)
    key('ctrl+z'); check('undo restores transform', abs(selected()['x']-old_x)<.01)
    key('ctrl+shift+z'); check('redo reapplies transform', selected()['x']>old_x)
    original = selected().copy()
    ox,oy=332+original['x'],315+original['y']
    ow,oh=original['width']*original['scaleX'],original['height']*original['scaleY']
    drag(ox+ow,oy+oh,ox+ow+55,oy+oh+12,'Shift_L')
    check('Shift resize preserves proportions', abs((selected()['width']*selected()['scaleX'])/(selected()['height']*selected()['scaleY'])-ow/oh)<.001)
    key('ctrl+z')
    click('duplicate'); check('duplicate selects new layer', len(layers()) == 2 and selected()['id'] != rectangle)
    click('layer-'+rectangle, 'Control_L')
    check('control toggle layer selection', len(state()['state']['selection']['ids']) == 2)
    click('group'); check('group selected layers', len(layers()) == 3 and selected()['content']['kind'] == 'group')
    group = selected()['id']
    click('add-paint'); paint = selected()['id']
    click('visibility-'+paint);check('layer visibility toggle', not selected()['visible']);click('visibility-'+paint)
    click('layer-'+rectangle,'Shift_L');check('Shift range selection',len(state()['state']['selection']['ids'])>1);click('layer-'+paint)
    source=bounds('layer-'+paint);target=bounds('layer-'+group)
    drag(source[0]+160,source[1]+20,target[0]+160,target[1]+20)
    check('drag layer into folder',selected()['parentId']==group)
    click('out-of-folder');check('move layer out of folder',selected()['parentId'] is None)
    rows=len(state()['state']['layerRows']);click('collapse-'+group);check('folder collapse',len(state()['state']['layerRows'])<rows);click('collapse-'+group)
    click('tool-brush'); field('brush-size','25'); field('brush-opacity','70'); field('hardness','45'); field('smoothing','25')
    check('brush controls', all(abs(state()['state'][k]-v)<.001 for k,v in [('brushSize',25),('brushOpacity',.7),('brushHardness',.45),('brushSmoothing',25)]))
    count=state()['state']['history']['undoCount']; drag(360,340,620,540)
    check('brush stroke is one undo', state()['state']['history']['undoCount']==count+1)
    count=state()['state']['history']['undoCount'];drag(450,410,1100,540)
    check('pointer release beyond canvas commits once',state()['state']['history']['undoCount']==count+1);key('ctrl+z')
    click('tool-eraser'); drag(490,400,510,445)
    click('mask-add'); check('mask added and targeted', selected()['mask'] is not None and state()['state']['paintTarget']=='mask')
    click('tool-brush'); drag(390,330,560,470)
    click('mask-reveal'); check('mask reveal control', state()['state']['maskMode']=='reveal')
    click('mask-link'); check('mask unlink control', selected()['mask']['linked'] is False)
    shot('02-mask-inspector.png')
    click('mask-enabled');check('mask disable',not selected()['mask']['enabled']);click('mask-enabled')
    click('paint-content'); check('return to layer content', state()['state']['paintTarget']=='content')
    choose('mask-source',0);check('live clipping source picker',selected()['maskSourceId'] is not None);click('clipping')
    click('tool-marquee'); drag(350,335,600,500)
    check('marquee creates selection', state()['state']['hasPixelSelection'])
    field('selection-amount','9',False); before=state()['state']['pixelSelectionBounds']; click('pixels-expand')
    check('selection expansion uses current form value', state()['state']['pixelSelectionBounds'] != before)
    field('feather','4',False); click('pixels-feather'); click('pixels-fill')
    shot('03-selection.png'); click('pixels-deselect')
    check('deselect pixels', not state()['state']['hasPixelSelection'])
    click('tool-lasso');x('mousemove',380,360,'mousedown',1)
    for px,py in [(570,360),(570,470),(380,470),(380,360)]: x('mousemove',px,py);time.sleep(.04)
    x('mouseup',1);time.sleep(.2);check('lasso polygon selection',state()['state']['hasPixelSelection']);key('ctrl+d')
    click('tool-crop');click('crop-square');click('crop-apply');time.sleep(.3)
    check('crop ratio and apply',state()['state']['document']['width']==state()['state']['document']['height']);key('ctrl+z');click('tool-move')
    click('foreground-picker'); field('color-hex','12abef',False); click('modal-cancel')
    check('color cancel preserves foreground', state()['state']['color'].lower()=='#a5b4fc')
    click('foreground-picker'); field('color-hex','12abef',False); shot('04-color-picker.png'); click('modal-apply')
    wait(lambda s:s['state']['color'].lower()=='#12abef')
    check('color picker commits hex', True)
    click('canvas-size'); click('canvas-locked'); field('canvas-width','480',False)
    shot('05-canvas-size.png'); click('modal-apply'); wait(lambda s:s['modal'] is None)
    check('canvas resize keeps original aspect ratio', state()['state']['document']['width']==480 and state()['state']['document']['height']==360)
    key('ctrl+z'); check('canvas resize undo', state()['state']['document']['width']==400)
    click('canvas-size');choose('canvas-unit',1);click('canvas-relative');click('canvas-locked');field('canvas-width','50',False);click('anchor-0');choose('canvas-fill',4);field('canvas-custom','#123456',False);click('modal-apply');wait(lambda s:s['modal'] is None and not s['busy'])
    check('relative percent canvas resize with custom fill',state()['state']['document']['width']==600 and state()['state']['document']['height']==450);key('ctrl+z')
    click('tool-text'); x('mousemove',400,350,'click',1);time.sleep(.25)
    check('text tool creates text layer', selected()['content']['kind']=='text')
    click('text-editor');key('ctrl+a');x('type','--clearmodifiers','Native text');key('shift+Return');x('type','--clearmodifiers','parity');key('Return')
    check('Shift Enter adds newline and Enter commits text',selected()['content']['text']=='Native text\nparity')
    click('text-editor');key('ctrl+a');x('type','--clearmodifiers','Native text parity');time.sleep(.2)
    click('field-font-size'); key('ctrl+a');x('type','32');key('Return')
    check('text editor and font size preserve each other', selected()['content']['text']=='Native text parity' and selected()['content']['fontSize']==32)
    choose('font',2);check('font selector',selected()['content']['fontFamily']=='monospace');choose('font',0)
    choose('blend',1);check('blend selector',selected()['blend']=='multiply');choose('blend',0)
    field('x','48');field('rotation','12');click('flip-x')
    check('transform fields and flip', selected()['x']==48 and selected()['rotation']==12 and selected()['flipX'])
    count=state()['state']['history']['undoCount'];left,top,width,height=bounds('slider-opacity');drag(left+width*.95,top+height/2,left+width*.4,top+height/2)
    check('opacity slider transaction', selected()['opacity']<.6 and state()['state']['history']['undoCount']==count+1)
    field('brightness','125');field('saturation','85');field('blur','2.5')
    check('typed adjustment values',selected()['brightness']==1.25 and selected()['saturation']==.85 and selected()['blur']==2.5)
    field('blur','0');click('lock');locked_x=selected()['x'];key('Right');check('locked layer rejects nudge',selected()['x']==locked_x);click('lock')
    click('text-editor');key('ctrl+a');key('ctrl+c');key('End');key('Return')
    shot('06-text-inspector.png')
    # Native file dialogs and real disk writes.
    project=out/'parity.picsie'
    click('save',scroll=False);portal(project)
    check('native save writes project and clears dirty',project.exists() and not state()['state']['history']['dirty'])
    click('text-editor');key('ctrl+shift+s');portal(out/'parity-as.picsie')
    check('Save As shortcut works with text focused',(out/'parity-as.picsie').exists())
    click('tool-move');click('export-png',scroll=False);portal(out/'parity.png')
    check('PNG export', (out/'parity.png').read_bytes()[:8]==b'\x89PNG\r\n\x1a\n')
    key('ctrl+alt+shift+s');portal(out/'parity.jpg')
    check('JPEG export shortcut', (out/'parity.jpg').read_bytes()[:2]==b'\xff\xd8')
    click('save-comp',scroll=False);portal(out/'parity.comp')
    check('Compositor package save', (out/'parity.comp').is_dir() and 'rasterized' in state()['notice'])
    click('import',scroll=False);portal(out/'parity.png',save=False)
    check('native image import',selected()['content']['kind']=='image')
    click('tool-move');key('ctrl+w');wait(lambda s:s['modal']=='close');shot('07-unsaved-close.png');click('modal-cancel')
    check('cancel close retains dirty document',state()['modal'] is None and state()['state']['history']['dirty'])
    x('windowsize',window_id,960,640);time.sleep(.4)
    check('minimum window canvas layout',state()['controls']['canvas']==[64.,98.,616.,514.])
    shot('08-minimum-size.png')
    x('windowsize',window_id,1280,860);time.sleep(.3)
    key('ctrl+w');wait(lambda s:s['modal']=='close');click('modal-apply');time.sleep(.6)
    end=time.monotonic()+10
    while window_exists(window_id) and time.monotonic()<end:time.sleep(.05)
    check('save on close writes and closes window',app.poll() is None and not window_exists(window_id))
    window_number=1;focus('Color studies.*Picsie');click('open')
    # The open dialog uses the original window; switch trace only after successful loading.
    portal(project,save=False)
    window_number=3;wait(lambda s:s.get('state') is not None,'reopen window');focus('GPUI parity.*Picsie')
    check('saved project reopens with text and layer structure',len(layers())==5 and any(l['content'].get('text')=='Native text parity' for l in layers()))
    shot('09-reopened.png')
    # The .comp directory picker is verified separately with mouse navigation;
    # GTK's location-entry keyboard behavior is unreliable without a window manager.
    click('add-paint');key('ctrl+q');wait(lambda s:s['modal']=='close');click('modal-cancel')
    check('cancel quit keeps application running',app.poll() is None and state()['modal'] is None)
    key('ctrl+q');wait(lambda s:s['modal']=='close');click('modal-discard')
    end=time.monotonic()+10
    while app.poll() is None and time.monotonic()<end:time.sleep(.05)
    check('discard and quit closes remaining clean windows',app.poll()==0)
    (out/'report.json').write_text(json.dumps({'checks':checks,'count':len(checks),'backend':'Linux X11 software Vulkan' if args.software else 'Linux X11','binary':args.binary},indent=2)+'\n')
    print(f'All {len(checks)} checks passed. Screenshots: {out}',flush=True)
except BaseException:
    try: shot('failure.png')
    except (OSError, subprocess.SubprocessError): pass
    raise
finally:
    if state():
        (out/'final-state.json').write_text(json.dumps(state(),indent=2)+'\n')
    for process in reversed(processes):
        try:os.killpg(process.pid,signal.SIGTERM)
        except ProcessLookupError:pass
    for process in processes:
        try:process.wait(timeout=5)
        except subprocess.TimeoutExpired:process.kill()
