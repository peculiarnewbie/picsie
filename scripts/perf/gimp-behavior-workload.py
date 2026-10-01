"""Local GIMP public-API workloads paired with performance_behaviors.rs.

Executed by GIMP's python-fu-eval interpreter, not ordinary CPython. No upstream
implementation or fixtures are copied. CPU timings include libgimp IPC and a
complete canvas-sized RGBA buffer read; they exclude UI and file encoding.
"""
import json
import os
from pathlib import Path
import time

from gi.repository import Gegl, Gimp, Gio


CASES = [
    'selection-fill', 'selection-clear',
    'selection-fill-feather20', 'selection-clear-feather20',
    'crop-retained', 'image-resize-half-linear', 'image-resize-half-nearest',
    'brush-hard100', 'eraser-hard100',
]
SIZES = [(1200, 800), (3600, 2400)]
output = Path(os.environ['PICSIE_PERF_OUTPUT'])
fixtures = Path(os.environ['PICSIE_PERF_FIXTURES'])
warmups = int(os.environ.get('PICSIE_PERF_WARMUPS', 5))
sample_count = int(os.environ.get('PICSIE_PERF_SAMPLES', 16))
wanted = os.environ.get('PICSIE_PERF_CASE')
output.mkdir(parents=True, exist_ok=True)

assert Gimp.context_set_foreground(Gegl.Color.new('#e53935'))
assert Gimp.context_set_opacity(100.)
assert Gimp.context_set_paint_mode(Gimp.LayerMode.NORMAL)
assert Gimp.context_set_antialias(True)
assert Gimp.context_set_feather(False)
assert Gimp.context_set_interpolation(Gimp.InterpolationType.LINEAR)
brush = Gimp.Brush.get_by_name('2. Hardness 100')
assert brush is not None, 'Required GIMP hard round brush is unavailable'
assert Gimp.context_set_brush(brush)
assert Gimp.context_set_brush_size(100.)
assert Gimp.context_set_brush_hardness(1.)
assert Gimp.context_set_brush_spacing(.015)
assert Gimp.context_enable_dynamics(False)
assert Gimp.context_get_brush_size() == 100.
assert Gimp.context_get_brush_hardness() == 1.
assert abs(Gimp.context_get_brush_spacing() - .015) < 1e-9

results = []
for width, height in SIZES:
    original = Gimp.file_load(Gimp.RunMode.NONINTERACTIVE,
                              Gio.File.new_for_path(str(fixtures/f'{width}.png')))
    assert original is not None
    for case in CASES:
        if wanted and case != wanted:
            continue
        assert Gimp.context_set_interpolation(
            Gimp.InterpolationType.NONE if case.endswith('nearest') else Gimp.InterpolationType.LINEAR)
        rows = []
        for iteration in range(warmups + sample_count):
            image = original.duplicate()
            assert image.undo_is_enabled(), 'History must remain enabled'
            layer = image.get_layers()[0]
            if not layer.has_alpha():
                assert layer.add_alpha()
            assert layer.has_alpha()
            if case.startswith('selection-'):
                assert image.select_rectangle(Gimp.ChannelOps.REPLACE,
                                              width/12, height/8, width/2, height/2)
                # Feather is metadata in Picsie but eager coverage in GIMP. Count
                # GIMP's materialization inside the timed operation as well.
            coordinates = [coordinate for i in range(61)
                           for coordinate in (width/12 + i*width/120, height/2)]
            started = time.perf_counter()
            if case.endswith('feather20'):
                assert Gimp.Selection.feather(image, 35.)
            if case.startswith('selection-fill'):
                assert layer.edit_fill(Gimp.FillType.FOREGROUND)
            elif case.startswith('selection-clear'):
                assert layer.edit_clear()
            elif case == 'crop-retained':
                # GIMP image.crop discards layer pixels. resize matches Compositor's
                # retained-content canvas crop: reposition layers and shrink canvas.
                assert image.resize(width*3//4, height*3//4, -width//12, -height//8)
            elif case in ['image-resize-half-linear', 'image-resize-half-nearest']:
                assert image.scale(width//2, height//2)
            elif case == 'brush-hard100':
                assert Gimp.paintbrush(layer, 0., coordinates,
                                      Gimp.PaintApplicationMode.CONSTANT, 0.)
            elif case == 'eraser-hard100':
                assert Gimp.eraser(layer, coordinates, Gimp.BrushApplicationMode.SOFT,
                                   Gimp.PaintApplicationMode.CONSTANT)
            else:
                raise AssertionError(case)
            command_ms = (time.perf_counter() - started)*1000
            out_width, out_height = image.get_width(), image.get_height()
            valid, offset_x, offset_y = layer.get_offsets()
            assert valid
            pixels = layer.get_buffer().get(
                Gegl.Rectangle.new(-offset_x, -offset_y, out_width, out_height),
                1., "R'G'B'A u8", Gegl.AbyssPolicy.NONE)
            total_ms = (time.perf_counter() - started)*1000
            assert len(pixels) == out_width*out_height*4
            at = ((out_height//2)*out_width + out_width//3)*4
            center = list(pixels[at:at+4])
            if case.startswith('selection-fill') or case.startswith('brush-'):
                assert center == [229, 57, 53, 255], (case, center)
            elif case.startswith('selection-clear') or case.startswith('eraser-'):
                assert center[3] == 0, (case, center)
            elif case == 'crop-retained':
                assert (out_width, out_height) == (width*3//4, height*3//4)
                assert (layer.get_width(), layer.get_height()) == (width, height)
                assert (offset_x, offset_y) == (-width//12, -height//8)
            else:
                assert (out_width, out_height) == (width//2, height//2)
                assert (layer.get_width(), layer.get_height()) == (out_width, out_height)
            assert list(pixels[:4]) == [101, 135, 255, 255], (case, list(pixels[:4]))
            if iteration == warmups:
                (output/f'{width}-{case}.rgba').write_bytes(pixels)
            if iteration >= warmups:
                rows.append(dict(command_ms=command_ms, total_ms=total_ms))
            image.delete()
        results.append(dict(width=width, height=height, case=case, samples=rows))
        print(f'Completed {width} {case}', flush=True)
    original.delete()
print('BEHAVIOR_RESULT '+json.dumps(dict(
    app='gimp', scope='Public API command with history plus complete canvas RGBA read. '
    'One raster layer means no extra projection composition is required. Includes '
    'libgimp IPC, excludes UI/GPU/encoding. Fresh image duplicate for each sample; '
    'feather materialization inside timer; setup and assertions outside timer.',
    brush_name=brush.get_name(), brush_spacing_percent=Gimp.context_get_brush_spacing()*100,
    results=results)), flush=True)
