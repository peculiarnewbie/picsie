"""Traced within-Picsie control: expanded versus collapsed folder rows.

Uses actual native controls, unchanged document pixels and the existing trace
metrics. This is diagnostic evidence; never pool it into untraced cohorts.
Compositor NativeLayerList.swift reuses cells and updates visible rows. The local
panel built every expanded row before the projection rewrite; it now virtualizes
rows in ui/layers.rs. Retained for reproducing that baseline. No app code changes.
"""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys
import time

sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location("navigation", ROOT / "scripts/compare-stress-navigation.py")
nav = importlib.util.module_from_spec(spec)
spec.loader.exec_module(nav)


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--output", required=True, type=Path)
    p.add_argument("--fixture", type=Path, default=ROOT / "artifacts/perf-stress-2026-10-01/fixtures/3600-1000-complex-overlap.picsie")
    p.add_argument("--binary", type=Path, default=ROOT / "crates/picsie-desktop/target/release/picsie-desktop")
    p.add_argument("--display", default=":97")
    a = p.parse_args()
    a.output = a.output.resolve()
    assert not a.output.exists(), "Choose a fresh output directory"
    a.output.mkdir(parents=True)
    tools = ROOT / "artifacts/selection-history/tools/usr"
    env = {**os.environ, "DISPLAY": a.display, "WINIT_UNIX_BACKEND": "x11",
           "VK_DRIVER_FILES": "/usr/share/vulkan/icd.d/radeon_icd.json", "MESA_VK_WSI_DEBUG": "sw",
           "PICSIE_GPU_DIAGNOSTICS": "1", "PICSIE_TRACE_DIR": str(a.output / "trace"),
           "XDG_CONFIG_HOME": str(a.output / "config"), "XDG_CACHE_HOME": str(a.output / "cache"),
           "PATH": str(tools / "bin") + os.pathsep + os.environ.get("PATH", ""),
           "LD_LIBRARY_PATH": str(tools / "lib")}
    env.pop("WAYLAND_DISPLAY", None)
    manifest = dict(scope=__doc__, source_sha256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
                    navigation_sha256=hashlib.sha256((ROOT / "scripts/compare-stress-navigation.py").read_bytes()).hexdigest(),
                    binary_sha256=hashlib.sha256(a.binary.read_bytes()).hexdigest(),
                    fixture_sha256=hashlib.sha256(a.fixture.read_bytes()).hexdigest())
    (a.output / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
    server = process = screen = None
    rows = []
    try:
        server = subprocess.Popen(["Xvfb", a.display, "-screen", "0", "1280x900x24", "-nolisten", "tcp"],
                                  env=env, stdout=(a.output / "xvfb.log").open("w"), stderr=subprocess.STDOUT, start_new_session=True)
        time.sleep(.5)
        assert server.poll() is None
        process = subprocess.Popen([str(a.binary), "--open", str(a.fixture)], env=env,
                                   stdout=(a.output / "app.log").open("w"), stderr=subprocess.STDOUT, start_new_session=True)
        def window():
            assert process.poll() is None
            matches = subprocess.run(["xdotool", "search", "--onlyvisible", "--name", "Picsie"], env=env, capture_output=True, text=True).stdout.splitlines()
            return matches[-1] if matches else None
        wid = nav.wait(window, "window")
        nav.comparison.xdo(env, "windowfocus", wid)
        screen = nav.Screen(a.display)
        screen.output = a.output
        screen.references = {}
        screen.tolerance = 0
        screen.resize(wid, 1245, 848)
        nav.comparison.xdo(env, "windowmove", wid, "0", "0")
        ox, oy = screen.origin(wid)
        roi = [ox + 66, oy + 94, 916, 714]
        trace = a.output / "trace/window-1.json"
        def snapshot():
            try:
                d = json.loads(trace.read_text())
                return d if d.get("state") else None
            except (OSError, ValueError):
                return None
        nav.idle(process.pid)
        nav.wait(snapshot, "first traced frame")
        screen.chord("h")
        screen.move(1100, 875)
        time.sleep(.25)
        def fit(): screen.chord("0", "Control_L")
        def actual(): screen.chord("1", "Control_L")
        fit(); nav.idle(process.pid); base = nav.settled(screen, roi)
        # A separate traced leaf reorder diagnoses the expensive engine frame.
        # It is not pooled with zoom or the untraced interaction cohort.
        before_document = snapshot()["state"]["document"]
        reorder_trace = []
        for case, key in [("adjacent-leaf", "bracketleft"), ("undo-leaf", "z")]:
            screen.chord(key, "Control_L")
            nav.idle(process.pid)
            frame = nav.settled(screen, roi)
            data = snapshot()
            assert data["sequence"] == data["submittedSequence"]
            reorder_trace.append(dict(case=case, trace_metrics=data["metrics"], frame_crc=frame))
            if case == "adjacent-leaf": assert frame != base
            else:
                assert frame == base
                assert data["state"]["document"] == before_document
        (a.output / "reorder-trace.json").write_text(json.dumps(reorder_trace, indent=2) + "\n")
        # Collapsing an active child's folder selects the folder by design.
        # Select that same folder before BOTH phases to isolate row expansion.
        data = snapshot()
        groups = {l["id"] for l in data["state"]["document"]["layers"] if l["content"]["kind"] == "group"}
        selected_group = next(r["id"] for r in data["state"]["layerRows"] if r["id"] in groups)
        bounds = data["controls"]["layer-" + selected_group]
        screen.move(ox + 1140, oy + bounds[1] + bounds[3] / 2)
        screen.button(True); screen.button(False)
        nav.wait(lambda: snapshot()["state"]["selection"]["ids"] == [selected_group], "select control folder")
        nav.idle(process.pid)
        screen.move(1100, 875)
        assert nav.settled(screen, roi) == base
        original_document = snapshot()["state"]["document"]
        original_history = snapshot()["state"]["history"]
        original_selection = snapshot()["state"]["selection"]
        actual(); nav.idle(process.pid); actual_ref = nav.settled(screen, roi)
        fit(); nav.idle(process.pid); assert nav.settled(screen, roi) == base
        for phase in ["expanded", "collapsed"]:
            if phase == "collapsed":
                collapsed = []
                for _ in range(100):
                    data = snapshot()
                    state = data["state"]
                    groups = {l["id"] for l in state["document"]["layers"] if l["content"]["kind"] == "group"}
                    remaining = [r["id"] for r in state["layerRows"] if r["id"] in groups and not r["collapsed"]]
                    if not remaining: break
                    visible = [(id, data["controls"].get("collapse-" + id)) for id in remaining]
                    visible = [(id, b) for id, b in visible if b and oy + 230 <= b[1] and b[1] + b[3] < oy + 770]
                    if not visible:
                        screen.move(ox + 1190, oy + 360)
                        for _ in range(5): screen.wheel(False)
                        nav.idle(process.pid)
                        continue
                    id, bounds = visible[0]
                    screen.move(ox + bounds[0] + bounds[2] / 2, oy + bounds[1] + bounds[3] / 2)
                    screen.button(True); screen.button(False)
                    nav.wait(lambda: any(r["id"] == id and r["collapsed"] for r in snapshot()["state"]["layerRows"]), "collapse folder")
                    nav.idle(process.pid)
                    collapsed.append(id)
                else: raise RuntimeError("Folder collapse did not finish")
                screen.move(1100, 875)
                assert snapshot()["state"]["document"] == original_document
                assert snapshot()["state"]["history"] == original_history
                assert snapshot()["state"]["selection"] == original_selection
                assert nav.settled(screen, roi) == base, "Collapse changed canvas pixels"
                (a.output / "collapsed-folders.json").write_text(json.dumps(collapsed, indent=2) + "\n")
            nav.comparison.screenshot(wid, a.output / (phase + ".png"), env)
            for iteration in range(5):
                for case, action, target, old in [("fit-to-100", actual, actual_ref, base), ("100-to-fit", fit, base, actual_ref)]:
                    row = nav.measure(screen, action, roi, target, old, process.pid)
                    nav.idle(process.pid)
                    data = snapshot()
                    assert data["state"]["document"] == original_document
                    assert data["state"]["history"] == original_history
                    assert data["state"]["selection"] == original_selection
                    if iteration >= 2:
                        rows.append(dict(phase=phase, case=case, visible_rows=len(data["state"]["layerRows"]),
                                         trace_metrics=data["metrics"], **row))
                        (a.output / "samples.partial.json").write_text(json.dumps(rows, indent=2) + "\n")
        (a.output / "results.json").write_text(json.dumps(dict(manifest=manifest, rows=rows, document_unchanged=True,
                                                              history_unchanged=True, selection_unchanged=True, exact_canvas_unchanged=True), indent=2) + "\n")
    finally:
        if screen: screen.close()
        nav.comparison.desktop.stop(process)
        nav.comparison.desktop.stop(server)


if __name__ == "__main__":
    main()
