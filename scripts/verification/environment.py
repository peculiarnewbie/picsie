"""Native test setup only; never document state or editor algorithms."""
import os
from pathlib import Path


def add_environment_arguments(parser):
    parser.add_argument('--tools', type=Path, help='Optional extracted tools prefix (bin/lib/share); otherwise use system tools')
    parser.add_argument('--software', action='store_true', help='Select Mesa lavapipe explicitly')
    parser.add_argument('--gpu-icd', type=Path, help='Explicit Vulkan ICD; otherwise keep the environment/default driver')


def native_environment(args, out, trace):
    env = {**os.environ, 'DISPLAY': args.display, 'PICSIE_TRACE_DIR': str(trace),
           'XDG_CONFIG_HOME': str(out / 'config'), 'XDG_DATA_HOME': str(out / 'data'),
           'XDG_CURRENT_DESKTOP': 'PicsieVerification', 'WINIT_UNIX_BACKEND': 'x11'}
    env.pop('WAYLAND_DISPLAY', None)
    if args.tools:
        tools = args.tools.resolve()
        env['PATH'] = str(tools / 'bin') + os.pathsep + env.get('PATH', '')
        env['LD_LIBRARY_PATH'] = str(tools / 'lib') + os.pathsep + env.get('LD_LIBRARY_PATH', '')
    if args.software and args.gpu_icd:
        raise SystemExit('Choose --software or --gpu-icd, not both')
    driver = args.gpu_icd
    if args.software:
        prefix = args.tools.resolve() if args.tools else Path('/usr')
        drivers = sorted((prefix / 'share/vulkan/icd.d').glob('lvp_icd*.json'))
        if not drivers:
            raise SystemExit('Mesa lavapipe ICD missing; install mesa-vulkan-drivers or supply --tools')
        driver = drivers[0]
    if driver:
        if not driver.is_file():
            raise SystemExit(f'Vulkan ICD missing: {driver}')
        env['VK_DRIVER_FILES'] = str(driver.resolve())
    return env


def backend_description(env):
    return {'window_system': 'Linux X11/Xvfb',
            'vulkan_icd': env.get('VK_DRIVER_FILES', env.get('VK_ICD_FILENAMES', 'system default')),
            'wsi_debug': env.get('MESA_VK_WSI_DEBUG', ''),
            'physical_display_latency': False}
