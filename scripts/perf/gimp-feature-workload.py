"""Local GIMP public-API workloads paired with performance_features.rs.

Executed by GIMP's python-fu-eval interpreter, not ordinary CPython. No upstream
implementation or fixtures are copied. CPU timings include libgimp IPC and a
complete canvas-sized RGBA buffer read; they exclude UI and file encoding.

Matched fixture constants (see the Rust example): 1200x800 and 3600x2400
canvases; bottom #e0d040 full-canvas; middle #3f7fbf 3/4-size rect offset by
1/8; top #c85050 full-canvas selected layer; mask cases reveal the left half.

Selection evolution workloads (see the Rust example header for the pinned
Compositor/GIMP sources) reuse a segmented fixture: opaque bottom plus a
transparent selected top layer carrying two separated opaque red squares
(side = width/8 at x = width/8 and x = width - width/8 - side, vertically
centered). Input shape, antialiasing, sampling scope, tolerance and alpha
semantics are matched explicitly with Picsie:
- invert/expand/contract start from an untimed center-half rectangle
  (select_rectangle REPLACE, feather off, antialias on); the timed call is
  Selection.invert / grow 5 / shrink 5. Axis-aligned rectangles are
  antialias-neutral on both engines. Known algorithm limit: Picsie's
  stroked-band resize rasterizes soft (antialiased) edges while GIMP
  grow/shrink keep hard edges with feather off; the paired coverage
  comparison quantifies that edge ring instead of claiming parity.
- wand cases click the left-square center with zero tolerance, point sample,
  selected layer only (sample-merged off, COMPOSITE criterion, transparent
  sampling off, antialias on, feather off, 4-connected flood to match
  Picsie's scanline flood). Contiguous uses select_contiguous_color;
  noncontiguous uses select_color with the sampled opaque red.
- layer-alpha uses select_item on the top layer (antialias on, feather off),
  matching Picsie's alpha-silhouette outline. Edge rasterization differs
  (Skia vs GIMP); quantified, not claimed as parity.

Measurement boundaries per sample add a coverage_ms stage for selections:
command_ms covers the single public selection call only; coverage_ms covers
the selection-mask buffer extraction into a .selcov channel dump (the
selected-coverage availability proof, separate from the document RGBA);
render_ms covers the destructive visible-layer merge; read_ms covers the
RGBA buffer extraction; total_ms is the end-to-end CPU availability
boundary. Unchanged document RGBA is never treated as the selection check.
All getter validation, geometry and pixel assertions run AFTER total_ms,
outside every timer. Setup (fresh duplicate, mask creation, composite-space
selection, brush context, input rectangle, wand context) is untimed, as are
the per-case restoration controls.

Recent-features workloads (batch 3) reuse the same stages. Group cases use
a folder with two raster children and time one item-transform call on the
group; the transform bakes resampling into the children, unlike Picsie's
live layer transforms (labeled representation difference, never an
equal-quality speed claim). Gradients time one linear FG-to-BG blend fill
with matched black/white endpoints; mask gradients additionally dump the
real mask coverage (.maskcov). Levels/Curves use the endorsed
non-destructive gimp:levels / gimp:curves drawable filters (filter-new +
perceptual TRC config + append in untimed setup; the timed call mutates the
live filter: low-input set or curve point add, plus update).

Measurement boundaries per sample: command_ms covers the single public
feature call only; render_ms covers the destructive visible-layer merge
(GIMP's projection/flatten step, extra work Picsie's retained composite does
not need); read_ms covers the RGBA buffer extraction; total_ms is the
end-to-end CPU availability boundary. All getter validation, geometry and
pixel assertions run AFTER total_ms, outside every timer. Setup (fresh
duplicate, mask creation, composite-space selection, brush context) is
untimed, as are the per-case undo-restoration controls. This measures cold
CPU command plus flatten availability, never UI or retained-preview latency.
GIMP history stays enabled; every timed command is undoable on its
per-sample duplicate, which is discarded afterward.

Opacity quality control: the top layer's composite space is set to
RGB_NONLINEAR (untimed setup), so normal-mode opacity composites in sRGB
like Picsie. Verified by probe: default AUTO compositing yields
[133, 113, 163] at the overlap point while RGB_NONLINEAR yields
[111, 111, 152], matching Picsie exactly.
"""
import json
import os
from pathlib import Path
import time

from gi.repository import Gegl, Gimp, Gio

CASES = [
    'opacity-commit', 'visibility-toggle',
    'blend-multiply', 'blend-screen',
    'mask-disabled', 'mask-paint',
    'select-inverse', 'select-expand5', 'select-contract5',
    'wand-contiguous', 'wand-noncontiguous', 'select-layer-alpha',
    'group-resize', 'group-rotate',
    'gradient-image-linear', 'gradient-mask-linear',
    'levels-update', 'curves-update',
]
SELECTION_CASES = {
    'select-inverse', 'select-expand5', 'select-contract5',
    'wand-contiguous', 'wand-noncontiguous', 'select-layer-alpha',
}
RECENT_CASES = {
    'group-resize', 'group-rotate',
    'gradient-image-linear', 'gradient-mask-linear',
    'levels-update', 'curves-update',
}
MASKCOVERAGE_CASES = {'gradient-mask-linear'}
SIZES = [(1200, 800), (3600, 2400)]
TOP = '#c85050'
MIDDLE = '#3f7fbf'
BOTTOM = '#e0d040'
GRAY = '#808080'
TOP_RGBA = [200, 80, 80, 255]
MIDDLE_RGBA = [63, 127, 191, 255]
BOTTOM_RGBA = [224, 208, 64, 255]
GRAY_RGBA = [128, 128, 128, 255]
WHITE_RGBA = [255, 255, 255, 255]
BLACK_RGBA = [0, 0, 0, 255]
# Levels black=64 maps gray 128 to ~85; curves mid lift maps it to 192.
LEVELS_GRAY = [85, 85, 85, 255]
CURVES_GRAY = [192, 192, 192, 255]
# 35% top over middle / bottom-only corner in sRGB, matching Picsie.
OVERLAP_OPACITY = [111, 111, 152, 255]
CORNER_OPACITY = [216, 163, 70, 255]
output = Path(os.environ['PICSIE_PERF_OUTPUT'])
warmups = int(os.environ.get('PICSIE_PERF_WARMUPS', 2))
sample_count = int(os.environ.get('PICSIE_PERF_SAMPLES', 4))
output.mkdir(parents=True, exist_ok=True)

assert Gimp.context_set_foreground(Gegl.Color.new('#000000'))
assert Gimp.context_set_opacity(100.)
assert Gimp.context_set_paint_mode(Gimp.LayerMode.NORMAL)
brush = Gimp.Brush.get_by_name('2. Hardness 100')
assert brush is not None, 'Required GIMP hard round brush is unavailable'
assert Gimp.context_set_brush(brush)
assert Gimp.context_set_brush_size(100.)
assert Gimp.context_set_brush_hardness(1.)
assert Gimp.context_set_brush_spacing(.015)
assert Gimp.context_enable_dynamics(False)


def solid_layer(image, name, color, width, height, offset_x, offset_y):
    assert Gimp.context_set_foreground(Gegl.Color.new(color))
    layer = Gimp.Layer.new(image, name, width, height,
                           Gimp.ImageType.RGBA_IMAGE, 100.,
                           Gimp.LayerMode.NORMAL)
    assert layer is not None
    assert image.insert_layer(layer, None, 0)
    assert layer.edit_fill(Gimp.FillType.FOREGROUND)
    # Void over GI; verify through getters instead (untimed setup).
    layer.set_offsets(offset_x, offset_y)
    valid, actual_x, actual_y = layer.get_offsets()
    assert valid and (actual_x, actual_y) == (offset_x, offset_y)
    return layer


def template(width, height):
    image = Gimp.Image.new(width, height, Gimp.ImageBaseType.RGB)
    assert image is not None
    Gimp.Selection.none(image)
    # Insert bottom-first so position 0 keeps the stack top, middle, bottom.
    solid_layer(image, 'Bottom', BOTTOM, width, height, 0, 0)
    solid_layer(image, 'Middle', MIDDLE, width*3//4, height*3//4,
                width//8, height//8)
    solid_layer(image, 'Top', TOP, width, height, 0, 0)
    Gimp.Selection.none(image)
    assert image.undo_is_enabled(), 'History must remain enabled'
    return image


def add_half_mask(image, layer, width, height):
    mask = layer.create_mask(Gimp.AddMaskType.WHITE)
    assert mask is not None
    assert layer.add_mask(mask)
    assert Gimp.context_set_foreground(Gegl.Color.new('#000000'))
    assert image.select_rectangle(Gimp.ChannelOps.REPLACE,
                                  width//2, 0, width - width//2, height)
    assert mask.edit_fill(Gimp.FillType.FOREGROUND)
    Gimp.Selection.none(image)
    assert layer.get_apply_mask()
    return mask


def selection_template(width, height):
    """Segmented selection fixture mirroring selection_fixture in Rust."""
    image = Gimp.Image.new(width, height, Gimp.ImageBaseType.RGB)
    assert image is not None
    Gimp.Selection.none(image)
    solid_layer(image, 'Bottom', BOTTOM, width, height, 0, 0)
    side = width//8
    layer = Gimp.Layer.new(image, 'Top', width, height,
                           Gimp.ImageType.RGBA_IMAGE, 100.,
                           Gimp.LayerMode.NORMAL)
    assert layer is not None
    assert image.insert_layer(layer, None, 0)
    assert layer.edit_fill(Gimp.FillType.TRANSPARENT)
    assert Gimp.context_set_foreground(Gegl.Color.new(TOP))
    for x0 in (width//8, width - width//8 - side):
        assert image.select_rectangle(Gimp.ChannelOps.REPLACE,
                                      x0, height//2 - side//2, side, side)
        assert layer.edit_fill(Gimp.FillType.FOREGROUND)
    Gimp.Selection.none(image)
    assert image.undo_is_enabled(), 'History must remain enabled'
    return image


def selection_input_rect(width, height):
    return (width//4, height//4, width//2, height//2)


def group_template(width, height):
    """Modest matched group fixture mirroring group_fixture in Rust: opaque
    base plus a folder carrying two raster children (same fractions)."""
    image = Gimp.Image.new(width, height, Gimp.ImageBaseType.RGB)
    assert image is not None
    Gimp.Selection.none(image)
    solid_layer(image, 'Bottom', BOTTOM, width, height, 0, 0)
    group = Gimp.GroupLayer.new(image, 'Group')
    assert group is not None
    assert image.insert_layer(group, None, 0)
    for name, color, w, h, ox, oy in (
            ('A', TOP, width//3, height//4, width//12, height//12),
            ('B', MIDDLE, width//6, height//3, width*2//3, height//3)):
        assert Gimp.context_set_foreground(Gegl.Color.new(color))
        child = Gimp.Layer.new(image, name, w, h,
                               Gimp.ImageType.RGBA_IMAGE, 100.,
                               Gimp.LayerMode.NORMAL)
        assert child is not None
        assert image.insert_layer(child, group, 0)
        assert child.edit_fill(Gimp.FillType.FOREGROUND)
        child.set_offsets(ox, oy)
        valid, actual_x, actual_y = child.get_offsets()
        assert valid and (actual_x, actual_y) == (ox, oy)
    Gimp.Selection.none(image)
    assert image.undo_is_enabled(), 'History must remain enabled'
    return image


def group_box(width, height):
    """Upright member bounding box, matching Picsie's group_box for
    axis-aligned members: min (w/12, h/12), max (5w/6, 2h/3)."""
    return (width/12, height/12, width*5/6, height*2/3)


def gradient_template(width, height):
    """Single mid-gray layer for the linear image gradient."""
    image = Gimp.Image.new(width, height, Gimp.ImageBaseType.RGB)
    assert image is not None
    Gimp.Selection.none(image)
    solid_layer(image, 'Base', GRAY, width, height, 0, 0)
    Gimp.Selection.none(image)
    assert image.undo_is_enabled(), 'History must remain enabled'
    return image


def mask_gradient_template(width, height):
    """Base plus a full-canvas red layer behind a solid white Reveal mask."""
    image = Gimp.Image.new(width, height, Gimp.ImageBaseType.RGB)
    assert image is not None
    Gimp.Selection.none(image)
    solid_layer(image, 'Bottom', BOTTOM, width, height, 0, 0)
    assert Gimp.context_set_foreground(Gegl.Color.new(TOP))
    layer = Gimp.Layer.new(image, 'Top', width, height,
                           Gimp.ImageType.RGBA_IMAGE, 100.,
                           Gimp.LayerMode.NORMAL)
    assert layer is not None
    assert image.insert_layer(layer, None, 0)
    assert layer.edit_fill(Gimp.FillType.FOREGROUND)
    mask = layer.create_mask(Gimp.AddMaskType.WHITE)
    assert mask is not None
    assert layer.add_mask(mask)
    assert layer.get_apply_mask()
    Gimp.Selection.none(image)
    assert image.undo_is_enabled(), 'History must remain enabled'
    return image


def adjustment_template(width, height):
    """Single opaque layer (mid-gray with a baked white square, same
    placement as the selection squares) so both engines' adjustment inputs
    are byte-identical."""
    image = Gimp.Image.new(width, height, Gimp.ImageBaseType.RGB)
    assert image is not None
    Gimp.Selection.none(image)
    side = width//8
    solid_layer(image, 'Base', GRAY, width, height, 0, 0)
    assert Gimp.context_set_foreground(Gegl.Color.new('#ffffff'))
    layers = {layer.get_name(): layer for layer in image.get_layers()}
    assert image.select_rectangle(Gimp.ChannelOps.REPLACE,
                                  width//8, height//2 - side//2, side, side)
    assert layers['Base'].edit_fill(Gimp.FillType.FOREGROUND)
    Gimp.Selection.none(image)
    assert image.undo_is_enabled(), 'History must remain enabled'
    return image


def setup_selection_input(image, case, layer, width, height):
    """Untimed input-shape and context setup for the selection cases."""
    assert Gimp.context_set_feather(False)
    assert Gimp.context_set_antialias(True)
    if case in ('select-inverse', 'select-expand5', 'select-contract5'):
        rx, ry, rw, rh = selection_input_rect(width, height)
        assert image.select_rectangle(Gimp.ChannelOps.REPLACE, rx, ry, rw, rh)
    elif case in ('wand-contiguous', 'wand-noncontiguous'):
        assert Gimp.context_set_sample_threshold(0.)
        assert Gimp.context_set_sample_merged(False)
        assert Gimp.context_set_sample_criterion(Gimp.SelectCriterion.COMPOSITE)
        assert Gimp.context_set_sample_transparent(False)
        assert Gimp.context_set_diagonal_neighbors(False)
    elif case == 'select-layer-alpha':
        pass
    else:
        raise AssertionError(case)


def feature_call(case, layer, mask, coordinates, image=None, seed=None,
                   group=None, filt=None):
    """The single timed public call. Void setters are called bare; every
    getter assertion happens after total_ms, outside the timer. Group
    transforms bake resampling into the children (unlike Picsie's live
    transforms); the filter updates mutate the live non-destructive
    control, mirroring Picsie's preview-inside-transaction."""
    seed_x, seed_y = seed if seed is not None else (0., 0.)
    if case == 'opacity-commit':
        layer.set_opacity(35.)
    elif case == 'visibility-toggle':
        layer.set_visible(False)
    elif case == 'blend-multiply':
        layer.set_mode(Gimp.LayerMode.MULTIPLY_LEGACY)
    elif case == 'blend-screen':
        layer.set_mode(Gimp.LayerMode.SCREEN_LEGACY)
    elif case == 'mask-disabled':
        layer.set_apply_mask(False)
    elif case == 'mask-paint':
        assert Gimp.paintbrush(mask, 0., coordinates,
                               Gimp.PaintApplicationMode.CONSTANT, 0.)
    elif case == 'select-inverse':
        assert Gimp.Selection.invert(image)
    elif case == 'select-expand5':
        assert Gimp.Selection.grow(image, 5)
    elif case == 'select-contract5':
        assert Gimp.Selection.shrink(image, 5)
    elif case == 'wand-contiguous':
        assert image.select_contiguous_color(
            Gimp.ChannelOps.REPLACE, layer, seed_x, seed_y)
    elif case == 'wand-noncontiguous':
        assert image.select_color(
            Gimp.ChannelOps.REPLACE, layer, Gegl.Color.new(TOP))
    elif case == 'select-layer-alpha':
        assert image.select_item(Gimp.ChannelOps.REPLACE, layer)
    elif case == 'group-resize':
        x0, y0, x1, y1 = coordinates
        cx, cy = (x0 + x1) / 2, (y0 + y1) / 2
        assert group.transform_scale(
            cx - 2 * (cx - x0), cy - 2 * (cy - y0),
            cx + 2 * (x1 - cx), cy + 2 * (y1 - cy)) is not None
    elif case == 'group-rotate':
        import math
        # Explicit box center (matching Picsie's box-center rotation), not
        # auto-center: the baked bounding box shifts under rotation, so an
        # auto-centered inverse would compound around a moved center.
        w, h = image.get_width(), image.get_height()
        x0, y0, x1, y1 = group_box(w, h)
        assert group.transform_rotate(
            math.radians(30.), False, (x0 + x1) / 2, (y0 + y1) / 2) is not None
    elif case == 'gradient-image-linear':
        x1, y1, x2, y2 = coordinates
        assert layer.edit_gradient_fill(Gimp.GradientType.LINEAR, 0.,
                                        False, 1, 0., False,
                                        x1, y1, x2, y2)
    elif case == 'gradient-mask-linear':
        x1, y1, x2, y2 = coordinates
        assert mask.edit_gradient_fill(Gimp.GradientType.LINEAR, 0.,
                                       False, 1, 0., False,
                                       x1, y1, x2, y2)
    elif case == 'levels-update':
        filt.get_config().set_property('low-input', 64. / 255.)
        filt.update()
    elif case == 'curves-update':
        curve = filt.get_config().get_property('curve')
        curve.set_curve_type(Gimp.CurveType.SMOOTH)
        curve.add_point(128. / 255., 192. / 255.)
        filt.update()
    else:
        raise AssertionError(case)


def check_sample(case, image, layer, width, height, pixels,
                   group_snapshot=None, pristine=None):
    out_width, out_height = image.get_width(), image.get_height()
    assert (out_width, out_height) == (width, height)
    assert len(pixels) == out_width*out_height*4
    overlap = ((height//2)*out_width + out_width//3)*4
    hidden = ((height//2)*out_width + out_width*3//4)*4
    painted = ((height//2)*out_width + out_width//4)*4
    corner = (10*out_width + (out_width - 10))*4
    if case == 'opacity-commit':
        assert layer.get_opacity() == 35.
        assert list(pixels[overlap:overlap+4]) == OVERLAP_OPACITY, (case, list(pixels[overlap:overlap+4]))
        assert list(pixels[corner:corner+4]) == CORNER_OPACITY, (case, list(pixels[corner:corner+4]))
    elif case == 'visibility-toggle':
        assert not layer.get_visible()
        assert list(pixels[overlap:overlap+4]) == MIDDLE_RGBA, (case, list(pixels[overlap:overlap+4]))
        assert list(pixels[corner:corner+4]) == BOTTOM_RGBA, (case, list(pixels[corner:corner+4]))
    elif case == 'blend-multiply':
        assert layer.get_mode() == Gimp.LayerMode.MULTIPLY_LEGACY
        assert list(pixels[overlap:overlap+4]) == [49, 40, 60, 255], (case, list(pixels[overlap:overlap+4]))
        assert list(pixels[corner:corner+4]) == [176, 65, 20, 255], (case, list(pixels[corner:corner+4]))
    elif case == 'blend-screen':
        assert layer.get_mode() == Gimp.LayerMode.SCREEN_LEGACY
        assert list(pixels[overlap:overlap+4]) == [214, 167, 211, 255], (case, list(pixels[overlap:overlap+4]))
        assert list(pixels[corner:corner+4]) == [248, 223, 124, 255], (case, list(pixels[corner:corner+4]))
    elif case == 'mask-disabled':
        assert not layer.get_apply_mask()
        assert list(pixels[hidden:hidden+4]) == TOP_RGBA, (case, list(pixels[hidden:hidden+4]))
        # The revealed left half is unaffected by disabling.
        assert list(pixels[overlap:overlap+4]) == TOP_RGBA, (case, list(pixels[overlap:overlap+4]))
    elif case == 'mask-paint':
        center = list(pixels[painted:painted+4])
        assert center == MIDDLE_RGBA, (case, center)
        # Far from the stroke the masked side still shows the middle.
        assert list(pixels[hidden:hidden+4]) == MIDDLE_RGBA, (case, list(pixels[hidden:hidden+4]))
    elif case == 'group-resize':
        # Snapshot before the destructive merge consumed the children.
        assert group_snapshot is not None
        assert len(group_snapshot) == 2, (case, group_snapshot)
        names = {entry[0] for entry in group_snapshot}
        assert names == {'A', 'B'}, (case, names)
        sizes = {entry[0]: (entry[1], entry[2]) for entry in group_snapshot}
        # The 1200px box corners are fractional (h/12), so GIMP's baked
        # resampling grid rounds member sizes by a pixel; doubling still
        # holds within 2px. Pixel truth comes from the paired comparison.
        for name, ew, eh in (('A', width//3*2, height//4*2),
                             ('B', width//6*2, height//3*2)):
            w, h = sizes[name]
            assert abs(w - ew) <= 2 and abs(h - eh) <= 2, (case, sizes)
        probe = ((height//8)*out_width + out_width//24)*4
        assert list(pixels[probe:probe+4]) == TOP_RGBA, (case, list(pixels[probe:probe+4]))
        probe_b = ((height//2)*out_width + out_width*23//24)*4
        assert list(pixels[probe_b:probe_b+4]) == MIDDLE_RGBA, (case, list(pixels[probe_b:probe_b+4]))
    elif case == 'group-rotate':
        assert group_snapshot is not None
        assert len(group_snapshot) == 2, (case, group_snapshot)
        # Baked rotation changes member footprints: require nontrivial
        # changed output against the pristine merge (exact angles live in
        # the resampled pixels, quantified by the paired comparison).
        assert pristine is not None
        changed = sum(1 for i in range(0, len(pixels), 4)
                      if pixels[i:i+4] != pristine[i:i+4])
        assert changed*1000 >= width*height, (case, changed)
    elif case == 'gradient-image-linear':
        left = list(pixels[((height//2)*out_width + 1)*4:((height//2)*out_width + 1)*4+4])
        right = list(pixels[((height//2)*out_width + out_width - 2)*4:((height//2)*out_width + out_width - 2)*4+4])
        middle = list(pixels[((height//2)*out_width + out_width//2)*4:((height//2)*out_width + out_width//2)*4+4])
        for channel in range(3):
            assert abs(left[channel] - 0) <= 4, (case, left)
            assert abs(right[channel] - 255) <= 4, (case, right)
            assert abs(middle[channel] - 128) <= 2, (case, middle)
        assert left[3] == 255 and right[3] == 255 and middle[3] == 255
    elif case == 'gradient-mask-linear':
        # Composite follows the mask coverage: base left, red right, with
        # sRGB (NON_LINEAR) compositing on this side. Windows match the
        # Rust driver exactly. GIMP
        # paints masks in linear light, so its 95% coverage sits lower than
        # Picsie's; loose bounds only, exactness comes from the paired
        # comparison.
        left = list(pixels[((height//2)*out_width + out_width*5//100)*4:((height//2)*out_width + out_width*5//100)*4+4])
        right = list(pixels[((height//2)*out_width + out_width*95//100)*4:((height//2)*out_width + out_width*95//100)*4+4])
        for channel in range(4):
            assert abs(left[channel] - BOTTOM_RGBA[channel]) <= 8, (case, left)
            assert abs(right[channel] - TOP_RGBA[channel]) <= 16, (case, right)
    elif case == 'levels-update':
        gray = list(pixels[((height//2)*out_width + out_width//2)*4:((height//2)*out_width + out_width//2)*4+4])
        for channel in range(3):
            assert abs(gray[channel] - 85) <= 3, (case, gray)
        assert gray[3] == 255
        side = width//8
        white = list(pixels[((height//2)*out_width + width//8 + side//2)*4:((height//2)*out_width + width//8 + side//2)*4+4])
        assert white == WHITE_RGBA, (case, white)
    elif case == 'curves-update':
        gray = list(pixels[((height//2)*out_width + out_width//2)*4:((height//2)*out_width + out_width//2)*4+4])
        for channel in range(3):
            assert abs(gray[channel] - 192) <= 3, (case, gray)
        assert gray[3] == 255
        side = width//8
        white = list(pixels[((height//2)*out_width + width//8 + side//2)*4:((height//2)*out_width + width//8 + side//2)*4+4])
        assert white == WHITE_RGBA, (case, white)


def check_selection_sample(case, image, width, height, mask):
    """Changed interior/exterior probes plus bounds on the materialized
    selection mask. Runs after total_ms, outside every timer."""
    assert len(mask) == width*height
    rx, ry, rw, rh = selection_input_rect(width, height)
    cx, cy = rx + rw//2, ry + rh//2
    side = width//8
    left = ((height//2)*width + (width//8 + side//2))
    right = ((height//2)*width + (width - width//8 - side//2))
    gap = ((height//2)*width + width//2)
    corner = 10*width + 10
    ok, non_empty, x1, y1, x2, y2 = Gimp.Selection.bounds(image)
    assert ok and non_empty, (case, 'selection unexpectedly empty')
    if case == 'select-inverse':
        assert (x1, y1, x2, y2) == (0, 0, width, height), (case, (x1, y1, x2, y2))
        assert Gimp.Selection.value(image, cx, cy) == 0, (case, 'former interior selected')
        assert mask[cy*width + cx] == 0
        assert Gimp.Selection.value(image, 10, 10) == 255, (case, 'former exterior missing')
        assert mask[corner] == 255
    elif case == 'select-expand5':
        assert (x1, y1, x2, y2) == (rx - 5, ry - 5, rx + rw + 5, ry + rh + 5), (case, (x1, y1, x2, y2))
        assert Gimp.Selection.value(image, cx, cy) == 255
        assert mask[cy*width + cx] == 255
        assert Gimp.Selection.value(image, rx - 3, ry + rh//2) == 255, (case, 'grown band missing')
        assert Gimp.Selection.value(image, 10, 10) == 0
    elif case == 'select-contract5':
        assert (x1, y1, x2, y2) == (rx + 5, ry + 5, rx + rw - 5, ry + rh - 5), (case, (x1, y1, x2, y2))
        assert Gimp.Selection.value(image, cx, cy) == 255
        assert mask[cy*width + cx] == 255
        assert Gimp.Selection.value(image, rx + 2, ry + rh//2) == 0, (case, 'trimmed edge kept')
        assert Gimp.Selection.value(image, 10, 10) == 0
    elif case == 'wand-contiguous':
        assert (x1, y1, x2, y2) == (width//8, height//2 - side//2,
                                    width//8 + side, height//2 + side//2), (case, (x1, y1, x2, y2))
        assert Gimp.Selection.value(image, width//8 + side//2, height//2) == 255
        assert mask[left] == 255
        assert Gimp.Selection.value(image, width - width//8 - side//2, height//2) == 0, (case, 'disconnected leak')
        assert mask[right] == 0
        assert mask[gap] == 0
    elif case == 'wand-noncontiguous':
        assert Gimp.Selection.value(image, width//8 + side//2, height//2) == 255
        assert mask[left] == 255
        assert Gimp.Selection.value(image, width - width//8 - side//2, height//2) == 255, (case, 'disconnected miss')
        assert mask[right] == 255
        assert mask[gap] == 0
    elif case == 'select-layer-alpha':
        assert Gimp.Selection.value(image, width//8 + side//2, height//2) == 255
        assert mask[left] == 255
        assert Gimp.Selection.value(image, width - width//8 - side//2, height//2) == 255
        assert mask[right] == 255
        assert mask[gap] == 0
    else:
        raise AssertionError(case)


def check_maskcoverage(case, width, height, maskcov):
    """Real mask-coverage assertions on the .maskcov channel dump.
    GIMP's linear-light mask ramp is deterministic at both sizes
    (verified: edges 0/255, 5% -> 1, middle 55, 95% -> 227). Picsie's
    encoded-space ramp differs by engine design (0/13/128/242/255); each
    side is checked against its own deterministic ramp and the paired
    comparison quantifies the gap — never an exact-mid parity claim."""
    assert len(maskcov) == width*height, (case, len(maskcov))
    mid = height//2
    assert maskcov[mid*width + 1] <= 1, (case, 'mask start not covered')
    assert maskcov[mid*width + width - 2] >= 254, (case, 'mask end not revealed')
    left = maskcov[mid*width + width*5//100]
    middle = maskcov[mid*width + width//2]
    right = maskcov[mid*width + width*95//100]
    assert abs(left - 1) <= 2, (case, left)
    assert abs(middle - 55) <= 4, (case, middle)
    assert abs(right - 227) <= 4, (case, right)


def pristine_merge(original, width, height, masked):
    """Untimed pristine reference: merge an untouched duplicate."""
    image = original.duplicate()
    layers = {layer.get_name(): layer for layer in image.get_layers()}
    if masked:
        add_half_mask(image, layers['Top'], width, height)
    merged = image.merge_visible_layers(Gimp.MergeType.CLIP_TO_IMAGE)
    pixels = merged.get_buffer().get(
        Gegl.Rectangle.new(0, 0, width, height),
        1., "R'G'B'A u8", Gegl.AbyssPolicy.NONE)
    image.delete()
    return bytes(pixels)


def read_selection_mask(image, width, height):
    """Materialized selection channel at canvas resolution (grayscale bytes).
    This is the selected-coverage availability proof, separate from the
    document RGBA merge below."""
    sel = image.get_selection()
    assert sel is not None
    return bytes(sel.get_buffer().get(
        Gegl.Rectangle.new(0, 0, width, height),
        1., 'Y u8', Gegl.AbyssPolicy.NONE))


def pristine_selection_coverage(original, case, width, height):
    """Untimed pristine input coverage: the rect mask for evolve cases, or
    the verified-empty selection for wand/layer-alpha (whose input is none).
    Returns None when the pristine state is empty."""
    if case in ('select-inverse', 'select-expand5', 'select-contract5'):
        image = original.duplicate()
        setup_selection_input(image, case, None, width, height)
        mask = read_selection_mask(image, width, height)
        image.delete()
        return mask
    image = original.duplicate()
    assert Gimp.Selection.is_empty(image), (case, 'selection fixture not pristine')
    image.delete()
    return None


def check_selection_restoration(case, original, width, height, pristine):
    """Untimed restoration control on a fresh duplicate. Batch GI exposes no
    single-step undo, so restoration uses a manually applied inverse or reset
    back to the pristine input coverage. Failures fail the run."""
    image = original.duplicate()
    layers = {layer.get_name(): layer for layer in image.get_layers()}
    layer = layers['Top']
    setup_selection_input(image, case, layer, width, height)
    side = width//8
    seed = (width//8 + side//2, height//2)
    feature_call(case, layer, None, [], image, seed)
    if case == 'select-inverse':
        # Boolean involution: a second invert restores the input exactly.
        assert Gimp.Selection.invert(image)
        method = 'double-invert-full-coverage'
    elif case == 'select-expand5':
        assert Gimp.Selection.shrink(image, 5)
        method = 'grow-then-shrink-full-coverage'
    elif case == 'select-contract5':
        # Shrink-then-grow (morphological opening) rounds the rectangle's
        # convex corners and does not restore the input byte-for-byte
        # (verified in the retained pilot sel-pilot-suite/gimp-1/app.log:
        # 'manual inverse did not restore pristine coverage'). The sound
        # control recreates the deterministic input rectangle instead.
        assert Gimp.Selection.none(image)
        setup_selection_input(image, case, layer, width, height)
        method = 'deselect-then-reselect-input-full-coverage'
    else:
        # Wand and layer-alpha start from no selection; deselecting restores
        # the empty input, verified through the empty predicate.
        assert Gimp.Selection.none(image)
        assert Gimp.Selection.is_empty(image), (case, 'deselect did not empty the channel')
        history_enabled = image.undo_is_enabled()
        image.delete()
        return {
            "method": "deselect-then-empty-predicate",
            "verified": True, "actual_undo_verified": False,
            "history_enabled": history_enabled,
            "scope": "empty input selection",
        }
    restored = read_selection_mask(image, width, height)
    assert restored == pristine, (case, 'manual inverse did not restore pristine coverage')
    history_enabled = image.undo_is_enabled()
    image.delete()
    return {
        "method": method,
        "verified": True, "actual_undo_verified": False,
        "history_enabled": history_enabled,
        "scope": "full restored selection coverage",
    }


def check_restoration(case, original, width, height, pristine):
    """Untimed restoration control on a fresh duplicate. Batch GI exposes no
    single-step undo (verified: no Image.undo attribute or gimp-image-undo
    PDB at e101dd19), so restoration is verified by an inverse round-trip
    back to the pristine merge. Failures fail the run."""
    image = original.duplicate()
    layers = {layer.get_name(): layer for layer in image.get_layers()}
    layer = layers['Top']
    mask = None
    if case in ('mask-disabled', 'mask-paint'):
        mask = add_half_mask(image, layer, width, height)
    if case == 'opacity-commit':
        layer.set_composite_space(Gimp.LayerColorSpace.RGB_NON_LINEAR)
    coordinates = [coordinate for i in range(61)
                   for coordinate in (width/12 + i*width/120, height/2)]
    feature_call(case, layer, mask, coordinates)
    # Inverse round-trip.
    if case == 'opacity-commit':
        layer.set_opacity(100.)
        assert layer.get_opacity() == 100.
    elif case == 'visibility-toggle':
        layer.set_visible(True)
        assert layer.get_visible()
    elif case == 'blend-multiply':
        layer.set_mode(Gimp.LayerMode.NORMAL)
        assert layer.get_mode() == Gimp.LayerMode.NORMAL
    elif case == 'blend-screen':
        layer.set_mode(Gimp.LayerMode.NORMAL)
        assert layer.get_mode() == Gimp.LayerMode.NORMAL
    elif case == 'mask-disabled':
        layer.set_apply_mask(True)
        assert layer.get_apply_mask()
    elif case == 'mask-paint':
        # White repaint cannot restore antialiased dab edges byte-for-byte
        # (white over black at partial coverage stays gray), so restoration
        # is verified at fully covered probes, not by full-frame equality.
        assert Gimp.context_set_foreground(Gegl.Color.new('#ffffff'))
        assert Gimp.paintbrush(mask, 0., coordinates,
                               Gimp.PaintApplicationMode.CONSTANT, 0.)
    merged = image.merge_visible_layers(Gimp.MergeType.CLIP_TO_IMAGE)
    pixels = merged.get_buffer().get(
        Gegl.Rectangle.new(0, 0, width, height),
        1., "R'G'B'A u8", Gegl.AbyssPolicy.NONE)
    if case == 'mask-paint':
        painted = ((height//2)*width + width//4)*4
        hidden = ((height//2)*width + width*3//4)*4
        assert list(pixels[painted:painted+4]) == TOP_RGBA, (case, list(pixels[painted:painted+4]))
        assert list(pixels[hidden:hidden+4]) == MIDDLE_RGBA, (case, list(pixels[hidden:hidden+4]))
    else:
        assert bytes(pixels) == pristine, (case, 'round-trip did not restore pristine merge')
    history_enabled = image.undo_is_enabled()
    image.delete()
    return {
        "method": "white-repaint-covered-probes" if case == "mask-paint" else "inverse-property-and-full-rgba",
        "verified": True, "actual_undo_verified": False,
        "history_enabled": history_enabled,
        "scope": "covered interior/exterior probes" if case == "mask-paint" else "full restored RGBA",
    }


def pristine_recent_merge(base, case, width, height):
    """Untimed pristine reference for a recent template: merge an untouched
    duplicate. The mask-gradient template already carries its solid white
    mask; nothing is added here."""
    image = base.duplicate()
    merged = image.merge_visible_layers(Gimp.MergeType.CLIP_TO_IMAGE)
    pixels = merged.get_buffer().get(
        Gegl.Rectangle.new(0, 0, width, height),
        1., "R'G'B'A u8", Gegl.AbyssPolicy.NONE)
    image.delete()
    return bytes(pixels)


def attach_levels_filter(layer):
    """Untimed identity gimp:levels control with perceptual TRC (matching
    Picsie's encoded-value tables, like the RGB_NONLINEAR composite-space
    alignment in batch 1). Returns the live filter."""
    filt = Gimp.DrawableFilter.new(layer, 'gimp:levels', 'levels-bench')
    assert filt is not None
    config = filt.get_config()
    config.set_property('trc', Gimp.TRCType.PERCEPTUAL)
    layer.append_filter(filt)
    filt.update()
    return filt


def attach_curves_filter(layer):
    """Untimed identity gimp:curves control with perceptual TRC."""
    filt = Gimp.DrawableFilter.new(layer, 'gimp:curves', 'curves-bench')
    assert filt is not None
    config = filt.get_config()
    config.set_property('trc', Gimp.TRCType.PERCEPTUAL)
    layer.append_filter(filt)
    filt.update()
    return filt


def setup_gradient_context(blend_space):
    """Untimed linear FG-to-BG black/white blend context, matched with
    Picsie's gradient settings on both sides. The blend space is explicit
    per case: RGB_PERCEPTUAL for the image gradient (sRGB ramp, matching
    the paintbrush tool preset default that the fill procedure actually
    reads), RGB_LINEAR for the mask gradient (linear Y channel intent).

    Verified limitation: `gimp-drawable-edit-gradient-fill` reads the
    paintbrush tool preset's blend space (default RGB_PERCEPTUAL, no batch
    setter), not this context value, so the mask mid-band still differs by
    engine design (see check_maskcoverage). The explicit setting documents
    intent; the paired comparison quantifies the result."""
    assert Gimp.context_set_foreground(Gegl.Color.new('#000000'))
    assert Gimp.context_set_background(Gegl.Color.new('#ffffff'))
    assert Gimp.context_set_gradient_fg_bg_rgb()
    assert Gimp.context_set_gradient_reverse(False)
    assert Gimp.context_set_gradient_blend_color_space(blend_space)


def check_restoration_recent(case, base, width, height, pristine):
    """Untimed recent restoration control on a fresh duplicate. Batch GI
    exposes no single-step undo, so restoration uses a manually applied
    inverse or reset. Baked group resampling is lossy: inverse corners
    restore member geometry, never resampled pixels, and the scope says so
    explicitly. Gradient repaints and filter resets restore byte-exact
    merges. Failures fail the run."""
    image = base.duplicate()
    layers = {layer.get_name(): layer for layer in image.get_layers()}
    if case in ('group-resize', 'group-rotate'):
        group = layers['Group']
        assert Gimp.context_set_interpolation(Gimp.InterpolationType.CUBIC)
        x0, y0, x1, y1 = group_box(width, height)
        if case == 'group-resize':
            cx, cy = (x0 + x1) / 2, (y0 + y1) / 2
            assert group.transform_scale(
                cx - 2 * (cx - x0), cy - 2 * (cy - y0),
                cx + 2 * (x1 - cx), cy + 2 * (y1 - cy)) is not None
            # Inverse corners restore member geometry exactly.
            assert group.transform_scale(x0, y0, x1, y1) is not None
            children = {child.get_name(): child for child in group.get_children()}
            assert set(children) == {'A', 'B'}, (case, set(children))
            for name, ew, eh in (('A', width//3, height//4),
                                 ('B', width//6, height//3)):
                w, h = children[name].get_width(), children[name].get_height()
                assert abs(w - ew) <= 2 and abs(h - eh) <= 2, (case, name, w, h)
            method, scope = 'inverse-corners-geometry', 'member geometry within 2px only; baked resampling is lossy, so this is not a raster restoration and no pixel restoration is claimed'
        else:
            import math
            x0, y0, x1, y1 = group_box(width, height)
            cx, cy = (x0 + x1) / 2, (y0 + y1) / 2
            assert group.transform_rotate(math.radians(30.), False, cx, cy) is not None
            # No inverse is attempted: baked rotation re-bboxes the padded
            # item buffer on every pass, so +30/-30 about a fixed center
            # yields 576x548 instead of 400x200 (retained pilot gi-group-diag5
            # with its DIAG5 log). Hierarchy preservation is the only sound
            # control here; geometry/pixel restoration is not claimed.
            children = {child.get_name() for child in group.get_children()}
            assert children == {'A', 'B'}, (case, children)
            method, scope = 'structure-preserved-no-sound-inverse', \
                'hierarchy preserved only; inverse rotation is not a reversible raster restoration (round-trip re-bboxes padded buffers, see gi-group-diag5), so no geometry or pixel restoration is claimed'
        history_enabled = image.undo_is_enabled()
        image.delete()
        return {"method": method, "verified": True,
                "actual_undo_verified": False,
                "history_enabled": history_enabled, "scope": scope}
    layer = layers['Base'] if case in (
        'gradient-image-linear', 'levels-update', 'curves-update',
    ) else layers.get('Top')
    if case == 'gradient-image-linear':
        setup_gradient_context(Gimp.GradientBlendColorSpace.RGB_PERCEPTUAL)
        assert layer.edit_gradient_fill(Gimp.GradientType.LINEAR, 0.,
                                        False, 1, 0., False,
                                        0.5, height / 2, width - 0.5, height / 2)
        assert Gimp.context_set_foreground(Gegl.Color.new(GRAY))
        assert layer.edit_fill(Gimp.FillType.FOREGROUND)
        method, scope = 'base-repaint-full-rgba', 'full restored RGBA'
    elif case == 'gradient-mask-linear':
        # The duplicate inherits the template's solid white mask; paint the
        # gradient onto it, then repaint white for the round-trip.
        mask = layer.get_mask()
        assert mask is not None
        layer.set_composite_space(Gimp.LayerColorSpace.RGB_NON_LINEAR)
        setup_gradient_context(Gimp.GradientBlendColorSpace.RGB_LINEAR)
        assert mask.edit_gradient_fill(Gimp.GradientType.LINEAR, 0.,
                                       False, 1, 0., False,
                                       0.5, height / 2, width - 0.5, height / 2)
        assert Gimp.context_set_foreground(Gegl.Color.new('#ffffff'))
        assert mask.edit_fill(Gimp.FillType.FOREGROUND)
        method, scope = 'mask-repaint-full-rgba', 'full restored RGBA'
    elif case == 'levels-update':
        filt = attach_levels_filter(layer)
        filt.get_config().set_property('low-input', 64. / 255.)
        filt.update()
        filt.get_config().set_property('low-input', 0.)
        filt.update()
        method, scope = 'filter-reset-full-rgba', 'full restored RGBA'
    elif case == 'curves-update':
        filt = attach_curves_filter(layer)
        curve = filt.get_config().get_property('curve')
        curve.set_curve_type(Gimp.CurveType.SMOOTH)
        index = curve.add_point(128. / 255., 192. / 255.)
        filt.update()
        curve.delete_point(index)
        filt.update()
        method, scope = 'filter-reset-full-rgba', 'full restored RGBA'
    else:
        raise AssertionError(case)
    merged = image.merge_visible_layers(Gimp.MergeType.CLIP_TO_IMAGE)
    pixels = merged.get_buffer().get(
        Gegl.Rectangle.new(0, 0, width, height),
        1., "R'G'B'A u8", Gegl.AbyssPolicy.NONE)
    assert bytes(pixels) == pristine, (case, 'round-trip did not restore pristine merge')
    history_enabled = image.undo_is_enabled()
    image.delete()
    return {
        "method": method,
        "verified": True, "actual_undo_verified": False,
        "history_enabled": history_enabled,
        "scope": scope,
    }


wanted = [case for case in os.environ.get('PICSIE_PERF_CASE', '').split(',') if case]
for case in wanted:
    assert case in CASES, case
results = []
checks = []
for width, height in SIZES:
    original = template(width, height)
    sel_original = selection_template(width, height)
    grp_original = group_template(width, height)
    grad_original = gradient_template(width, height)
    mgrad_original = mask_gradient_template(width, height)
    adj_original = adjustment_template(width, height)
    for case in CASES:
        if wanted and case not in wanted:
            continue
        selection_case = case in SELECTION_CASES
        recent_case = case in RECENT_CASES
        if selection_case:
            base = sel_original
        elif case in ('group-resize', 'group-rotate'):
            base = grp_original
        elif case == 'gradient-image-linear':
            base = grad_original
        elif case == 'gradient-mask-linear':
            base = mgrad_original
        elif case in ('levels-update', 'curves-update'):
            base = adj_original
        else:
            base = original
        # Untimed pristine reference for recent cases (one merge per
        # case/size, shared across samples and restoration).
        recent_pristine = pristine_recent_merge(base, case, width, height) \
            if recent_case else None
        rows = []
        for iteration in range(warmups + sample_count):
            # Untimed setup: fresh duplicate, mask, composite space, brush,
            # selection input shape and wand context.
            image = base.duplicate()
            layers = {layer.get_name(): layer for layer in image.get_layers()}
            layer = layers['Top'] if 'Top' in layers else layers.get('Base')
            mask = None
            group = layers.get('Group')
            filt = None
            if case in ('mask-disabled', 'mask-paint'):
                mask = add_half_mask(image, layer, width, height)
            if case == 'opacity-commit':
                layer.set_composite_space(Gimp.LayerColorSpace.RGB_NON_LINEAR)
            if case == 'mask-paint':
                assert Gimp.context_set_foreground(Gegl.Color.new('#000000'))
            if selection_case:
                setup_selection_input(image, case, layer, width, height)
            if case in ('group-resize', 'group-rotate'):
                assert Gimp.context_set_interpolation(Gimp.InterpolationType.CUBIC)
            if case in ('gradient-image-linear', 'gradient-mask-linear'):
                if case == 'gradient-mask-linear':
                    # The duplicate inherits the template's solid white mask.
                    mask = layer.get_mask()
                    assert mask is not None
                    # sRGB compositing for the fractional mask, matching
                    # Picsie (same control as opacity-commit).
                    layer.set_composite_space(Gimp.LayerColorSpace.RGB_NON_LINEAR)
                    setup_gradient_context(Gimp.GradientBlendColorSpace.RGB_LINEAR)
                else:
                    setup_gradient_context(Gimp.GradientBlendColorSpace.RGB_PERCEPTUAL)
            if case == 'levels-update':
                filt = attach_levels_filter(layer)
            if case == 'curves-update':
                filt = attach_curves_filter(layer)
            coordinates = [coordinate for i in range(61)
                           for coordinate in (width/12 + i*width/120, height/2)]
            if case in ('group-resize', 'group-rotate'):
                coordinates = group_box(width, height)
            elif case in ('gradient-image-linear', 'gradient-mask-linear'):
                coordinates = (0.5, height/2, width - 0.5, height/2)
            side = width//8
            seed = (width//8 + side//2, height//2)
            started = time.perf_counter()
            feature_call(case, layer, mask, coordinates, image, seed,
                         group=group, filt=filt)
            command_ms = (time.perf_counter() - started)*1000
            # Materialized selection coverage availability, separate from the
            # document RGBA merge below. Layer/mask cases have no selection
            # output; their coverage stage is defined as zero.
            if selection_case:
                selcov = read_selection_mask(image, width, height)
                coverage_ms = (time.perf_counter() - started)*1000 - command_ms
            else:
                selcov, coverage_ms = None, 0.
            # Pre-merge availability reads. They must precede the destructive
            # merge, so they join coverage_ms (the materialized-availability
            # stage, mirroring the selection boundary): render_ms below keeps
            # its existing formula and older cases stay exactly at 0.0.
            if case in MASKCOVERAGE_CASES:
                # Real mask coverage availability, separate from the RGBA
                # merge below (mirrors the .selcov boundary for selections).
                assert mask is not None
                mark = time.perf_counter()
                maskcov = bytes(mask.get_buffer().get(
                    Gegl.Rectangle.new(0, 0, width, height),
                    1., 'Y u8', Gegl.AbyssPolicy.NONE))
                coverage_ms += (time.perf_counter() - mark)*1000
            else:
                maskcov = None
            # Group hierarchy must be snapshotted before the destructive
            # merge below consumes the children. These are validation
            # metadata reads, not feature work: they run on a separate
            # stage timer (metadata_ms) that is untimed — excluded from
            # every stage and from total_ms below. Older cases perform no
            # such reads (0.0, boundaries unchanged).
            metadata_ms = 0.
            premerge_group_snapshot = None
            if case in ('group-resize', 'group-rotate'):
                assert group is not None
                mark = time.perf_counter()
                premerge_group_snapshot = [
                    (child.get_name(), child.get_width(),
                     child.get_height(), child.get_offsets())
                    for child in group.get_children()]
                metadata_ms = (time.perf_counter() - mark)*1000
            merged = image.merge_visible_layers(Gimp.MergeType.CLIP_TO_IMAGE)
            assert merged is not None
            # The measured render window includes the untimed metadata reads
            # above, so they are subtracted here: the reported stages sum
            # exactly to total_ms. Older cases have metadata_ms 0.0.
            render_ms = (time.perf_counter() - started)*1000 - command_ms - coverage_ms - metadata_ms
            valid, offset_x, offset_y = merged.get_offsets()
            assert valid
            out_width, out_height = image.get_width(), image.get_height()
            pixels = merged.get_buffer().get(
                Gegl.Rectangle.new(-offset_x, -offset_y, out_width, out_height),
                1., "R'G'B'A u8", Gegl.AbyssPolicy.NONE)
            read_ms = (time.perf_counter() - started)*1000 - command_ms - coverage_ms - render_ms - metadata_ms
            # Exact CPU total: wall time minus the untimed metadata reads.
            # Older cases have metadata_ms 0.0, so their totals are
            # unchanged wall-clock stage sums.
            total_ms = (time.perf_counter() - started)*1000 - metadata_ms
            # All validation below runs after total_ms, outside every timer.
            # Group hierarchy uses the pre-merge snapshot above: the
            # destructive merge consumed the children.
            group_snapshot = None
            if case in ('group-resize', 'group-rotate'):
                assert group is not None
                group_snapshot = premerge_group_snapshot
            if selection_case:
                check_selection_sample(case, image, width, height, selcov)
            else:
                check_sample(case, image, layer, width, height, pixels,
                             group_snapshot=group_snapshot,
                             pristine=recent_pristine)
            if case in MASKCOVERAGE_CASES:
                check_maskcoverage(case, width, height, maskcov)
            if iteration == warmups:
                # Only the first measured sample per case/size/trial is
                # dumped; coverage itself is materialized and validated on
                # every sample, and the paired comparison uses trial-1 dumps.
                (output/f'{width}-{case}.rgba').write_bytes(pixels)
                if selection_case:
                    (output/f'{width}-{case}.selcov').write_bytes(selcov)
                if case in MASKCOVERAGE_CASES:
                    (output/f'{width}-{case}.maskcov').write_bytes(maskcov)
            if iteration >= warmups:
                row = dict(command_ms=command_ms, coverage_ms=coverage_ms,
                           render_ms=render_ms, read_ms=read_ms, total_ms=total_ms)
                if metadata_ms:
                    # Group-only stage: untimed validation-metadata reads,
                    # excluded from total_ms above.
                    row['metadata_ms'] = metadata_ms
                rows.append(row)
            image.delete()
        # Untimed restoration controls; failures fail the run.
        if selection_case:
            pristine = pristine_selection_coverage(sel_original, case, width, height)
            checks.append(dict(width=width, height=height, case=case,
                               restoration=check_selection_restoration(case, sel_original, width, height, pristine)))
        elif recent_case:
            checks.append(dict(width=width, height=height, case=case,
                               restoration=check_restoration_recent(case, base, width, height, recent_pristine)))
        else:
            pristine = pristine_merge(original, width, height, case in ('mask-disabled', 'mask-paint'))
            checks.append(dict(width=width, height=height, case=case,
                               restoration=check_restoration(case, original, width, height, pristine)))
        results.append(dict(width=width, height=height, case=case, samples=rows))
        print(f'Completed {width} {case}', flush=True)
    original.delete()
    sel_original.delete()
    grp_original.delete()
    grad_original.delete()
    mgrad_original.delete()
    adj_original.delete()
print('FEATURE_RESULT '+json.dumps(dict(
    app='gimp', scope='Public API feature command (command_ms) with history, '
    'plus destructive visible-layer merge/projection (render_ms) and complete '
    'canvas RGBA read (read_ms); total_ms is the end-to-end CPU availability '
    'boundary. Includes libgimp IPC, excludes UI/GPU/encoding and any warm '
    'retained preview. Fresh image duplicate per sample; mask creation, '
    'nonlinear composite-space selection and brush context are untimed setup; '
    'getter validation, assertions and untimed manual-inverse restoration controls (actual Undo unavailable) '
    'outside timers. Selection cases add materialized selection-coverage availability every sample (coverage_ms); '
    'only the first measured sample per case/size/trial is dumped to a .selcov channel (paired comparison uses '
    'trial-1 dumps); unchanged document RGBA is never the selection check. '
    'Recent group cases time one item-transform call on the folder (baked resampling, unlike live layer '
    'transforms); gradients time one linear FG-to-BG fill with matched endpoints, and the mask gradient '
    'additionally materializes real mask coverage once per sample in coverage_ms (same channel validated and '
    'dumped; trial-1 dumps compared); Levels/Curves mutate the live '
    'non-destructive filter (low-input set or curve point add plus update) with identity-filter setup untimed. '
    'Group hierarchy snapshot getters run on a separate untimed metadata_ms stage excluded from every stage and '
    'from total_ms (total is wall minus metadata; older cases have metadata_ms 0.0 and unchanged totals). '
    'The mask fill reads the paintbrush tool preset blend space (default perceptual, no batch setter), so the '
    'mask mid-band differs by engine design; the top layer composites sRGB (NON_LINEAR) on the mask path.',
    brush_name=brush.get_name(), checks=checks,
    results=results)), flush=True)
