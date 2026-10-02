import test from "node:test";
import assert from "node:assert/strict";
import { mkdtemp, readFile, rm, stat, readdir, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { createRequire } from "node:module";
import { createCanvas, loadImage } from "@napi-rs/canvas";
import { Editor } from "../src/engine/editor.ts";
import { CanvasSizeDraft } from "../src/ui/canvas-size-draft.ts";
import type { Command, EditorState } from "../src/engine/types.ts";
const require = createRequire(import.meta.url);
const native: typeof import("../src/engine/native-api") = require("../native/picsie.node");
const session = () => new Editor({ kind: "new", name: "Native", width: 200, height: 160 });
async function temporary(run: (path: string) => Promise<void>) {
  const directory = await mkdtemp(join(tmpdir(), "picsie-rust-test-"));
  try {
    await run(directory);
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
}
async function pixel(path: string, x: number, y: number) {
  const image = await loadImage(path);
  const canvas = createCanvas(image.width, image.height);
  canvas.getContext("2d").drawImage(image, 0, 0);
  return [...canvas.getContext("2d").getImageData(x, y, 1, 1).data];
}

test("actual addon batches pointer input, renders native pixels and restores history", async () =>
  temporary(async (dir) => {
    const editor = session();
    try {
      editor.viewport = { width: 200, height: 160, zoom: 1, pan: { x: 0, y: 0 } };
      editor.setTool("brush");
      editor.setColor("#ff0000");
      editor.brushSize = 10;
      editor.pointer("down", { x: 20, y: 20 });
      for (let x = 21; x < 80; x++) editor.pointer("move", { x, y: 20 });
      editor.pointer("up", { x: 80, y: 20 });
      assert.equal(editor.history.undoCount, 1);
      const png = join(dir, "brush.png");
      await editor.export(png, "png");
      assert.deepEqual(await pixel(png, 50, 20), [255, 0, 0, 255]);
      assert.ok(!JSON.stringify(editor.document).includes("points"));
      editor.undo();
      assert.equal(editor.document.layers.length, 0);
      editor.redo();
      assert.equal(editor.document.layers.length, 1);
    } finally {
      editor.close();
    }
  }));

test("Rust masks, transforms, canvas resize, save/reopen and JPEG work across the bridge", async () =>
  temporary(async (dir) => {
    const e = session();
    try {
      e.viewport = { width: 200, height: 160, zoom: 1, pan: { x: 0, y: 0 } };
      e.setTool("rectangle");
      e.setColor("#ff0000");
      e.pointer("down", { x: 20, y: 20 });
      e.pointer("up", { x: 120, y: 120 });
      e.addMask();
      e.setTool("brush");
      e.brushSize = 30;
      e.pointer("down", { x: 60, y: 60 });
      e.pointer("up", { x: 60, y: 60 });
      const png = join(dir, "masked.png");
      await e.export(png, "png");
      assert.deepEqual(await pixel(png, 60, 60), [0, 0, 0, 0]);
      e.updateLayer({ rotation: 37, flipX: true });
      await e.resizeCanvas({ width: 220, height: 180, anchor: 4, fill: "#ffffff" });
      const project = join(dir, "edited.picsie");
      await e.save(project);
      assert.equal(e.history.dirty, false);
      const reopened = await Editor.open(project);
      try {
        assert.equal((await readFile(project, "utf8")).includes('"format":"picsie"'), true);
        assert.deepEqual(reopened.document, e.document);
        await e.export(png, "png");
        const second = join(dir, "reopened.png");
        await reopened.export(second, "png");
        assert.deepEqual(await readFile(second), await readFile(png));
        const jpeg = join(dir, "flattened.jpg");
        await reopened.export(jpeg, "jpeg");
        assert.deepEqual(await pixel(jpeg, 0, 0), [255, 255, 255, 255]);
      } finally {
        reopened.close();
      }
    } finally {
      e.close();
    }
  }));

test("native folder, pixel selection, crop, and Compositor package commands round-trip", async () =>
  temporary(async (dir) => {
    const e = session();
    try {
      e.viewport = { width: 200, height: 160, zoom: 1, pan: { x: 0, y: 0 } };
      e.setTool("rectangle");
      e.setColor("#ff0000");
      e.pointer("down", { x: 10, y: 10 });
      e.pointer("up", { x: 90, y: 90 });
      const child = e.selectedId!;
      e.addGroup();
      const folder = e.selectedId!;
      e.select(child);
      e.moveToGroup(folder);
      assert.equal(e.document.layers.find((layer) => layer.id === child)?.parentId, folder);
      assert.equal(e.layerRows.find((row) => row.id === child)?.depth, 1);
      e.select(folder);
      e.updateLayer({ opacity: 0.5 });
      e.setTool("marquee");
      e.pointer("down", { x: 20, y: 20 });
      e.pointer("up", { x: 40, y: 40 });
      assert.deepEqual(e.pixelSelectionBounds, { x: 20, y: 20, width: 20, height: 20 });
      e.select(child);
      e.clearSelectedPixels();
      const png = join(dir, "selected.png");
      await e.export(png, "png");
      assert.deepEqual(await pixel(png, 30, 30), [0, 0, 0, 0]);
      assert.deepEqual(await pixel(png, 50, 50), [255, 0, 0, 128]);
      e.setTool("crop");
      e.pointer("down", { x: 200, y: 160 });
      e.pointer("up", { x: 120, y: 120 });
      e.commitCrop();
      assert.equal(e.document.width, 120);
      await e.export(png, "png");
      const project = join(dir, "edited.comp");
      await e.save(project);
      assert.equal((await stat(project)).isDirectory(), true);
      const manifest = JSON.parse(await readFile(join(project, "manifest.json"), "utf8"));
      assert.equal(manifest.format, "com.compositor.project");
      assert.equal(manifest.version, 8);
      const reopened = await Editor.open(project);
      try {
        assert.equal(reopened.document.layers.length, e.document.layers.length);
        assert.equal(
          reopened.document.layers.find((layer) => layer.id === child)?.parentId,
          folder,
        );
        const second = join(dir, "reopened.png");
        await reopened.export(second, "png");
        assert.deepEqual(await readFile(second), await readFile(png));
      } finally {
        reopened.close();
      }
    } finally {
      e.close();
    }
  }));

test("feathered selections soften what they clip across the real addon", async () =>
  temporary(async (dir) => {
    const e = session();
    try {
      e.viewport = { width: 200, height: 160, zoom: 1, pan: { x: 0, y: 0 } };
      e.setTool("rectangle");
      e.setColor("#ff0000");
      e.pointer("down", { x: 10, y: 10 });
      e.pointer("up", { x: 150, y: 150 });
      e.setTool("marquee");
      e.featherSelection(6);
      assert.equal(e.pixelSelectionFeather, null);
      e.pointer("down", { x: 40, y: 40 });
      e.pointer("up", { x: 100, y: 100 });
      assert.equal(e.pixelSelectionFeather, 0);
      e.featherSelection(6);
      e.featherSelection(8);
      assert.equal(e.pixelSelectionFeather, 10);
      assert.throws(() => e.featherSelection(251), /between 1 and 250/);
      e.clearSelectedPixels();
      const png = join(dir, "feathered.png");
      await e.export(png, "png");
      assert.deepEqual(await pixel(png, 70, 70), [0, 0, 0, 0]);
      assert.deepEqual(await pixel(png, 14, 70), [255, 0, 0, 255]);
      const soft: number[] = [];
      for (let x = 28; x < 52; x++) soft.push((await pixel(png, x, 70))[3] ?? -1);
      assert.ok(
        soft.filter((alpha) => alpha > 8 && alpha < 247).length >= 4,
        `the cleared pixels have a hard edge: ${soft}`,
      );
      e.deselectPixels();
      e.featherSelection(6);
      assert.equal(e.pixelSelectionFeather, null);
    } finally {
      e.close();
    }
  }));

test("pixel selection history restores coverage, feather and edits through the actual addon", async () =>
  temporary(async (dir) => {
    const e = session();
    try {
      e.viewport = { width: 200, height: 160, zoom: 1, pan: { x: 0, y: 0 } };
      e.setTool("rectangle");
      e.setColor("#ff0000");
      e.pointer("down", { x: 10, y: 10 });
      e.pointer("up", { x: 150, y: 150 });
      const count = e.history.undoCount;
      e.setTool("marquee");
      e.pointer("down", { x: 40, y: 40 });
      for (let x = 41; x < 100; x++) e.pointer("move", { x, y: x });
      e.pointer("up", { x: 100, y: 100 });
      const bounds = { x: 40, y: 40, width: 60, height: 60 };
      assert.deepEqual(e.pixelSelectionBounds, bounds);
      assert.equal(e.history.undoCount, count + 1);
      assert.equal(e.history.undoLabel, "Rectangular Marquee");
      e.featherSelection(6);
      e.clearSelectedPixels();
      const png = join(dir, "cleared.png");
      await e.export(png, "png");
      const cleared = await readFile(png);
      assert.deepEqual(await pixel(png, 70, 70), [0, 0, 0, 0]);
      const alpha = (await pixel(png, 40, 70))[3]!;
      assert.ok(alpha > 8 && alpha < 247);
      e.deselectPixels();
      e.undo();
      assert.deepEqual(e.pixelSelectionBounds, bounds);
      assert.equal(e.pixelSelectionFeather, 6);
      e.undo();
      await e.export(png, "png");
      assert.deepEqual(await pixel(png, 70, 70), [255, 0, 0, 255]);
      e.undo();
      assert.equal(e.pixelSelectionFeather, 0);
      // The bridge flushes a queued move before the Escape command. Cancellation
      // must keep both the completed selection and the pending redo branch.
      e.pointer("down", { x: 110, y: 110 });
      e.pointer("move", { x: 130, y: 130 });
      e.cancelPixelSelection();
      e.pointer("up", { x: 130, y: 130 });
      assert.deepEqual(e.pixelSelectionBounds, bounds);
      assert.equal(e.history.canRedo, true);
      e.redo();
      e.redo();
      await e.export(png, "png");
      assert.deepEqual(await readFile(png), cleared);
      e.redo();
      assert.equal(e.pixelSelectionBounds, null);
      const beforeNoop = e.history.undoCount;
      e.deselectPixels();
      assert.equal(e.history.undoCount, beforeNoop);
      e.undo();
      e.cancelPixelSelection();
      assert.equal(e.pixelSelectionBounds, null);
      assert.equal(e.history.canRedo, false);
      assert.ok(!JSON.stringify(e.document).includes('"pixels"'));
    } finally {
      e.close();
    }
  }));

test("async canvas resize and crop restore selection atomically on undo", async () => {
  const e = session();
  try {
    e.viewport = { width: 200, height: 160, zoom: 1, pan: { x: 0, y: 0 } };
    e.selectAllPixels();
    e.featherSelection(6);
    const bounds = e.pixelSelectionBounds;
    const count = e.history.undoCount;
    await e.resizeCanvas({ width: 100, height: 80, anchor: 4 });
    assert.equal(e.pixelSelectionBounds, null);
    assert.equal(e.history.undoCount, count + 1);
    e.undo();
    assert.equal(e.document.width, 200);
    assert.deepEqual(e.pixelSelectionBounds, bounds);
    assert.equal(e.pixelSelectionFeather, 6);
    e.redo();
    assert.equal(e.document.width, 100);
    assert.equal(e.pixelSelectionBounds, null);
    e.undo();
    e.viewport = { width: 200, height: 160, zoom: 1, pan: { x: 0, y: 0 } };
    e.setTool("crop");
    e.pointer("down", { x: 200, y: 160 });
    e.pointer("up", { x: 120, y: 120 });
    e.commitCrop();
    assert.equal(e.document.width, 120);
    assert.equal(e.pixelSelectionBounds, null);
    e.undo();
    assert.deepEqual(e.pixelSelectionBounds, bounds);
    assert.equal(e.pixelSelectionFeather, 6);
    e.redo();
    assert.equal(e.document.width, 120);
    assert.equal(e.pixelSelectionBounds, null);
  } finally {
    e.close();
  }
});

test("text editing and slider edits cross the bridge as typed commands", async () =>
  temporary(async (dir) => {
    const reference = session();
    try {
      reference.viewport = { width: 200, height: 160, zoom: 1, pan: { x: 0, y: 0 } };
      reference.setTool("text");
      reference.pointer("down", { x: 20, y: 20 });
      reference.pointer("up", { x: 20, y: 20 });
      const content = reference.selected!.content;
      assert.equal(content.kind, "text");
      if (content.kind !== "text") throw new Error("Expected the Rust text draft");
      reference.beginPropertyEdit("Edit text");
      reference.updateLayer({ content: { ...content, text: "QuickGUI draft", fontSize: 24 } });
      assert.equal(reference.history.undoCount, 0);
      reference.finishGesture();
      assert.equal(reference.selected?.content.kind, "text");
      assert.equal(reference.history.undoCount, 1);
      const path = join(dir, "reference-text.picsie");
      await reference.save(path);
      const reopened = await Editor.open(path);
      try {
        assert.deepEqual(reopened.document.layers, reference.document.layers);
      } finally {
        reopened.close();
      }
    } finally {
      reference.close();
    }
    const e = new Editor({ kind: "demo" });
    try {
      const text = e.document.layers.find((layer) => layer.content.kind === "text")!;
      const id = text.id;
      assert.equal(e.textEditRequests, 0);
      e.editText({ id });
      assert.equal(e.selectedId, id);
      assert.equal(e.textEditRequests, 1);
      e.viewport = {
        width: e.document.width,
        height: e.document.height,
        zoom: 1,
        pan: { x: 0, y: 0 },
      };
      e.editText({ point: { x: text.x + 12, y: text.y + 12 } });
      assert.equal(e.textEditRequests, 2);
      // A slider drag previews every step but commits as one undo entry.
      const before = e.history.undoCount;
      e.beginPropertyEdit("Edit layer opacity");
      e.updateLayer({ opacity: 0.8 });
      e.updateLayer({ opacity: 0.2 });
      e.finishGesture();
      assert.equal(e.history.undoCount, before + 1);
      e.undo();
      assert.equal(e.document.layers.find((layer) => layer.id === id)?.opacity, 1);
      const png = join(dir, "text.png");
      await e.export(png, "png");
      assert.ok((await stat(png)).size > 0);
    } finally {
      e.close();
    }
  }));

test("file import is atomic, owns its assets, and metadata excludes all raster data", async () =>
  temporary(async (dir) => {
    const e = session();
    try {
      const input = join(dir, "red.png");
      const canvas = createCanvas(20, 20);
      canvas.getContext("2d").fillStyle = "#ff0000";
      canvas.getContext("2d").fillRect(0, 0, 20, 20);
      await writeFile(input, await canvas.encode("png"));
      await assert.rejects(e.importImages([input, join(dir, "missing.png")]));
      assert.equal(e.document.layers.length, 0);
      assert.equal(e.history.canUndo, false);
      await e.importImages([input]);
      await rm(input);
      const snapshot = JSON.stringify(e.document);
      assert.ok(!snapshot.includes("base64") && !snapshot.includes("strokes"));
      assert.ok(snapshot.length < 2000);
      const output = join(dir, "imported.png");
      await e.export(output, "png");
      assert.deepEqual(await pixel(output, 100, 80), [255, 0, 0, 255]);
    } finally {
      e.close();
    }
  }));

test("saving an earlier revision cannot incorrectly mark a newer edit clean", async () =>
  temporary(async (dir) => {
    const raw = new native.NativeEditor(JSON.stringify({ kind: "demo" }));
    try {
      raw.dispatch(JSON.stringify({ type: "nudge", delta: { x: 5, y: 0 } }));
      const saving = raw.save(join(dir, "earlier.picsie"));
      raw.dispatch(JSON.stringify({ type: "nudge", delta: { x: 5, y: 0 } }));
      await saving;
      assert.equal(JSON.parse(raw.snapshot()).history.dirty, true);
      raw.dispatch(JSON.stringify({ type: "undo" }));
      assert.equal(JSON.parse(raw.snapshot()).history.dirty, false);
    } finally {
      raw.close();
    }
  }));

test("preview transport is uncompressed, bounded, and released with its window", async () => {
  const e = session();
  let directory: string;
  const preview = await e.preview();
  directory = dirname(preview);
  try {
    assert.equal((await readFile(preview)).subarray(0, 4).toString("hex"), "49492a00");
    for (let i = 0; i < 10; i++) {
      e.pan({ x: 1, y: 0 });
      await e.preview();
    }
    assert.ok((await readdir(directory)).length <= 8);
    assert.ok((await stat(await e.preview())).size > 936 * 734 * 4);
  } finally {
    e.close();
  }
  await assert.rejects(stat(directory));
  assert.throws(() => e.fit(), /closed/);
});

test("two windows have independent state and a pending render cannot revive a closed editor", async () => {
  const a = session(),
    b = session();
  a.addPaintLayer();
  assert.equal(b.document.layers.length, 0);
  const rendering = a.preview();
  a.close();
  await assert.rejects(rendering, /closed/);
  b.addGradient();
  assert.equal(b.document.layers.length, 1);
  b.close();
});

test("legacy project fixtures open in the real addon", async () => {
  for (const name of ["legacy-editing", "legacy-native"]) {
    const e = await Editor.open(resolve(`tests/fixtures/${name}.electropic`));
    try {
      assert.equal(
        (await readFile(resolve(`tests/fixtures/${name}.electropic`), "utf8")).includes(
          '"format":"electropic"',
        ),
        true,
      );
      assert.ok(e.document.layers.length >= 6);
      assert.ok((await stat(await e.preview())).size > 54);
    } finally {
      e.close();
    }
  }
});

test("native eyedropper samples the composite through viewport coordinates", async () => {
  const e = session();
  try {
    e.viewport = { width: 200, height: 160, zoom: 1, pan: { x: 0, y: 0 } };
    e.setTool("rectangle");
    e.setColor("#ff0000");
    e.pointer("down", { x: 20, y: 20 });
    e.pointer("up", { x: 100, y: 100 });
    e.setColor("#000000");
    await e.sampleColor({ x: 50, y: 50 });
    assert.equal(e.color, "#ff0000");
    await e.sampleColor({ x: -10, y: -10 });
    assert.equal(e.color, "#ff0000");
  } finally {
    e.close();
  }
});

test("native shape styles, cycling, lines and package records cross the bridge", async () =>
  temporary(async (dir) => {
    const e = session();
    try {
      e.viewport = { width: 200, height: 160, zoom: 1, pan: { x: 0, y: 0 } };
      e.setTool("rectangle");
      e.setColor("#ff0000");
      e.setShapeCornerRadius(8);
      e.pointer("down", { x: 10, y: 10 });
      e.pointer("up", { x: 50, y: 40 });
      const rect = e.selected!;
      assert.equal(rect.name, "Rectangle 1");
      assert.equal(rect.content.kind, "shape");
      if (rect.content.kind !== "shape") throw new Error("expected shape");
      assert.equal(rect.content.shape, "rectangle");
      e.cycleShapeKind();
      e.setTool("line");
      e.setShapeLineWidth(6);
      e.pointer("down", { x: 60, y: 10 });
      e.pointer("up", { x: 100, y: 10 });
      const line = e.selected!;
      assert.equal(line.name, "Line 1");
      if (line.content.kind !== "shape") throw new Error("expected line");
      assert.equal(line.content.line_width, 6);
      assert.ok(line.content.line_start && line.content.line_end);
      const project = join(dir, "shapes.comp");
      await e.save(project);
      const manifest = JSON.parse(await readFile(join(project, "manifest.json"), "utf8"));
      const shapes = manifest.layers.filter((l: any) => l.shape);
      assert.equal(shapes.length, 2);
      assert.equal(shapes[0].shape.kind, "Rectangle");
      assert.equal(shapes[0].shape.cornerRadius, 8);
      assert.equal(shapes[1].shape.kind, "Line");
      const reopened = await Editor.open(project);
      try {
        assert.equal(reopened.document.layers.length, 2);
        const png = join(dir, "shapes.png");
        await e.export(png, "png");
        assert.deepEqual(await pixel(png, 30, 25), [255, 0, 0, 255]);
      } finally {
        reopened.close();
      }
    } finally {
      e.close();
    }
  }));

test("native gradient tool previews, applies and cancels through the real addon", async () =>
  temporary(async (dir) => {
    const e = session();
    try {
      e.viewport = { width: 200, height: 160, zoom: 1, pan: { x: 0, y: 0 } };
      e.setTool("rectangle");
      e.setColor("#ff0000");
      e.pointer("down", { x: 10, y: 10 });
      e.pointer("up", { x: 110, y: 30 });
      e.setTool("gradient");
      e.setGradientStyle("foreground-to-background");
      e.setColor("#000000");
      const before: number = e.history.undoCount;
      e.pointer("down", { x: 10.5, y: 20 });
      e.pointer("move", { x: 110.5, y: 20 });
      e.pointer("up", { x: 110.5, y: 20 });
      assert.equal(e.history.undoCount, before);
      e.commitGradient();
      assert.equal(e.history.undoCount, before + 1);
      const png = join(dir, "gradient.png");
      await e.export(png, "png");
      const start = await pixel(png, 10, 20);
      assert.ok(Math.abs((start[0] ?? -100)) <= 3 && start[3] === 255);
      e.undo();
      assert.equal(e.history.undoCount, before);
      e.setTool("gradient");
      e.pointer("down", { x: 10.5, y: 20 });
      e.pointer("up", { x: 10.5, y: 20 });
      e.cancelGradient();
      assert.equal(e.history.undoCount, before);
    } finally {
      e.close();
    }
  }));

test("pending gradient stays out of addon save/export until Apply", async () =>
  temporary(async (dir) => {
    const e = session();
    try {
      e.viewport = { width: 200, height: 160, zoom: 1, pan: { x: 0, y: 0 } };
      e.setTool("rectangle");
      e.setColor("#ff0000");
      e.pointer("down", { x: 10, y: 10 });
      e.pointer("up", { x: 110, y: 30 });
      e.setTool("gradient");
      e.setGradientStyle("foreground-to-background");
      e.setColor("#000000");
      e.pointer("down", { x: 10.5, y: 20 });
      e.pointer("move", { x: 110.5, y: 20 });
      e.pointer("up", { x: 110.5, y: 20 });
      // The line is pending: canvas export and project save must keep the base.
      const pending = join(dir, "pending.png");
      await e.export(pending, "png");
      assert.deepEqual(await pixel(pending, 10, 20), [255, 0, 0, 255]);
      const project = join(dir, "pending.picsie");
      await e.save(project);
      const reopened = await Editor.open(project);
      try {
        const clean = join(dir, "reopened.png");
        await reopened.export(clean, "png");
        assert.deepEqual(await pixel(clean, 10, 20), [255, 0, 0, 255]);
      } finally {
        reopened.close();
      }
      // Apply bakes the gradient; the same export then shows it.
      e.commitGradient();
      const applied = join(dir, "applied.png");
      await e.export(applied, "png");
      const start = await pixel(applied, 10, 20);
      assert.ok(Math.abs((start[0] ?? -100)) <= 3 && start[3] === 255);
    } finally {
      e.close();
    }
  }));

test("canvas form preserves original ratio across relative and percent edits", () => {
  const draft = new CanvasSizeDraft({ width: 400, height: 200 });
  draft.relative = true;
  draft.locked = true;
  draft.set(100, true);
  assert.equal(draft.width, 500);
  assert.equal(draft.height, 250);
  assert.equal(draft.displayed(false), 50);
  draft.unit = "percent";
  assert.equal(draft.displayed(true), 25);
  draft.set(-50, true);
  assert.equal(draft.width, 200);
  assert.equal(draft.height, 100);
  assert.equal(draft.valid, true);
  draft.set(-100, true);
  assert.equal(draft.valid, false);
});

test("stale native imports and canvas jobs cannot overwrite a newer document", async () =>
  temporary(async (dir) => {
    const raw = new native.NativeEditor(JSON.stringify({ kind: "demo" }));
    try {
      const input = join(dir, "source.png");
      await writeFile(input, await createCanvas(100, 100).encode("png"));
      const importing = raw.importImages([input]);
      raw.dispatch(JSON.stringify({ type: "nudge", delta: { x: 1, y: 0 } }));
      await assert.rejects(importing, /changed during import/);
      assert.equal(JSON.parse(raw.snapshot()).document.layers.length, 6);
      const resizing = raw.resizeCanvas(
        JSON.stringify({ width: 1400, height: 1000, anchor: 4, fill: "#ffffff" }),
      );
      raw.dispatch(JSON.stringify({ type: "nudge", delta: { x: 1, y: 0 } }));
      await assert.rejects(resizing, /changed during canvas resize/);
      assert.equal(JSON.parse(raw.snapshot()).document.width, 1200);
    } finally {
      raw.close();
    }
  }));

test("Rust validates untrusted commands and malformed projects without losing the active document", async () =>
  temporary(async (dir) => {
    const e = session();
    try {
      e.addPaintLayer();
      const before = e.document;
      assert.throws(() => e.updateLayer({ scaleX: 0 }), /transform/);
      await assert.rejects(e.resizeCanvas({ width: 8192, height: 8192, anchor: 4 }), /megapixel/);
      assert.deepEqual(e.document, before);
      const invalid = join(dir, "invalid.picsie");
      await writeFile(invalid, '{"version":999}');
      await assert.rejects(Editor.open(invalid), /Invalid Picsie/);
      assert.deepEqual(e.document, before);
    } finally {
      e.close();
    }
  }));

test("selection fill, inverse, expand and contract preserve native coverage and undo", async () =>
  temporary(async (dir) => {
    const e = session();
    try {
      e.viewport = { width: 200, height: 160, zoom: 1, pan: { x: 0, y: 0 } };
      e.addPaintLayer();
      e.setTool("marquee");
      e.pointer("down", { x: 40, y: 40 });
      e.pointer("up", { x: 60, y: 60 });
      e.expandSelection(5);
      assert.deepEqual(e.pixelSelectionBounds, { x: 35, y: 35, width: 30, height: 30 });
      e.contractSelection(8);
      assert.deepEqual(e.pixelSelectionBounds, { x: 43, y: 43, width: 14, height: 14 });
      e.featherSelection(3);
      e.setColor("#ff0000");
      e.fillSelection();
      const output = join(dir, "selection-ops.png");
      await e.export(output, "png");
      assert.equal((await pixel(output, 50, 50))[3], 255);
      assert.ok((await pixel(output, 42, 50))[3]! > 0);
      e.undo();
      await e.export(output, "png");
      assert.deepEqual(await pixel(output, 50, 50), [0, 0, 0, 0]);
      e.redo();
      e.selectAllPixels();
      e.invertSelection();
      assert.equal(e.hasPixelSelection, true);
      assert.equal(e.pixelSelectionBounds, null);
      assert.equal(e.canEditPixels, false);
      const count = e.history.undoCount;
      e.fillSelection();
      e.clearSelectedPixels();
      assert.equal(e.history.undoCount, count);
      assert.equal(e.document.layers.length, 1);
      e.invertSelection();
      assert.deepEqual(e.pixelSelectionBounds, { x: 0, y: 0, width: 200, height: 160 });
    } finally {
      e.close();
    }
  }));

test("native brush tips cap opacity, Shift joins endpoints, and raster assets reopen", async () =>
  temporary(async (dir) => {
    const e = session();
    try {
      e.viewport = { width: 200, height: 160, zoom: 1, pan: { x: 0, y: 0 } };
      e.setTool("brush");
      e.setColor("#ff0000");
      e.brushSize = 20;
      e.brushHardness = 1;
      e.brushSmoothing = 0;
      e.brushOpacity = 0.5;
      e.pointer("down", { x: 20, y: 40 });
      e.pointer("move", { x: 150, y: 40 });
      e.pointer("move", { x: 20, y: 40 });
      e.pointer("up", { x: 150, y: 40 });
      assert.equal(e.history.undoCount, 1);
      const output = join(dir, "brush-tip.png");
      await e.export(output, "png");
      assert.equal((await pixel(output, 80, 40))[3], 128);
      e.pointer("down", { x: 150, y: 120 }, { shift: true });
      e.pointer("up", { x: 150, y: 120 }, { shift: true });
      await e.export(output, "png");
      assert.equal((await pixel(output, 150, 80))[3], 128);
      e.undo();
      await e.export(output, "png");
      assert.equal((await pixel(output, 150, 80))[3], 0);
      e.redo();
      e.brushHardness = 0.35;
      e.brushSmoothing = 20;
      assert.equal(e.brushHardness, 0.35);
      assert.equal(e.brushSmoothing, 20);
      const project = join(dir, "brush-tip.picsie");
      await e.save(project);
      const reopened = await Editor.open(project);
      try {
        await reopened.export(output, "png");
        assert.equal((await pixel(output, 150, 80))[3], 128);
      } finally {
        reopened.close();
      }
      assert.ok(!JSON.stringify(e.document).includes("base64"));
    } finally {
      e.close();
    }
  }));

test("native clipping, folder mask paint, package persistence and source deletion agree", async () =>
  temporary(async (dir) => {
    const e = session();
    try {
      e.viewport = { width: 200, height: 160, zoom: 1, pan: { x: 0, y: 0 } };
      e.setTool("brush");
      e.setColor("#000000");
      e.brushSize = 80;
      e.brushOpacity = 0.5;
      e.pointer("down", { x: 80, y: 80 });
      e.pointer("up", { x: 80, y: 80 });
      const base = e.selectedId!;
      e.addPaintLayer();
      e.setColor("#ff0000");
      e.fillSelection();
      const top = e.selectedId!;
      assert.equal(e.canToggleClipping, true);
      e.toggleClippingMask();
      assert.equal(e.selected?.maskSourceId, base);
      const output = join(dir, "clipped.png");
      await e.export(output, "png");
      assert.deepEqual(await pixel(output, 80, 80), [255, 0, 0, 128]);
      assert.equal((await pixel(output, 10, 10))[3], 0);
      e.select(base, "toggle");
      e.groupSelected();
      const folder = e.selectedId!;
      e.addMask();
      e.brushOpacity = 1;
      e.brushSize = 10;
      e.pointer("down", { x: 80, y: 80 });
      e.pointer("up", { x: 80, y: 80 });
      await e.export(output, "png");
      assert.equal((await pixel(output, 80, 80))[3], 0);
      for (const extension of ["picsie", "comp"]) {
        const path = join(dir, `clipped.${extension}`);
        await e.save(path);
        const reopened = await Editor.open(path);
        try {
          assert.equal(reopened.document.layers.find((l) => l.id === top)?.maskSourceId, base);
          assert.ok(reopened.document.layers.find((l) => l.id === folder)?.mask);
          await reopened.export(output, "png");
          assert.equal((await pixel(output, 80, 80))[3], 0);
        } finally {
          reopened.close();
        }
      }
      e.resetMask("reveal");
      e.select(base);
      e.remove();
      assert.equal(e.document.layers.find((l) => l.id === top)?.maskSourceId, undefined);
      await e.export(output, "png");
      assert.deepEqual(await pixel(output, 80, 80), [255, 0, 0, 128]);
      e.undo();
      assert.equal(e.document.layers.find((l) => l.id === top)?.maskSourceId, base);
    } finally {
      e.close();
    }
  }));

test("actual addon executes selection, transform, merge and image size in native workers", async () =>
  temporary(async (dir) => {
    const raw = new native.NativeEditor(
      JSON.stringify({ kind: "new", name: "Workflows", width: 80, height: 40 }),
    );
    const dispatch = (command: Command): EditorState =>
      JSON.parse(raw.dispatch(JSON.stringify(command)));
    const worker = async (command: Command): Promise<EditorState> =>
      JSON.parse(await raw.dispatchAsync(JSON.stringify(command)));
    const pointer = (phase: "down" | "up", x: number, y: number) =>
      dispatch({
        type: "pointer",
        samples: [
          {
            phase,
            point: { x, y },
            modifiers: { shift: false, alt: false, control: false, meta: false },
          },
        ],
      });
    try {
      dispatch({
        type: "setViewport",
        viewport: { width: 80, height: 40, zoom: 1, pan: { x: 0, y: 0 } },
      });
      dispatch({ type: "setTool", tool: "rectangle" });
      dispatch({ type: "setColor", color: "#ff0000" });
      pointer("down", 0, 0);
      pointer("up", 40, 40);
      dispatch({ type: "setTool", tool: "marquee" });
      pointer("down", 5, 5);
      pointer("up", 15, 15);
      assert.throws(() => dispatch({ type: "layerViaCopy" }), /dispatchAsync/);
      let state = await worker({ type: "layerViaCopy" });
      assert.equal(state.document.layers.length, 2);
      assert.equal(state.document.layers[1]!.x, 5);
      dispatch({ type: "undo" });
      const before = raw.snapshot();
      state = await worker({ type: "beginTransform" });
      assert.equal(state.transformActive, true);
      dispatch({ type: "updateLayer", patch: { x: 25 } });
      assert.throws(() => raw.save(join(dir, "unfinished.picsie")), /Apply or cancel/);
      dispatch({ type: "cancelTransform" });
      assert.deepEqual(JSON.parse(raw.snapshot()).document, JSON.parse(before).document);
      const count = JSON.parse(raw.snapshot()).history.undoCount;
      await worker({ type: "beginTransform" });
      dispatch({ type: "updateLayer", patch: { x: 25 } });
      state = await worker({ type: "commitTransform" });
      assert.equal(state.history.undoCount, count + 1);
      assert.deepEqual(state.pixelSelectionBounds, { x: 25, y: 5, width: 10, height: 10 });
      const png = join(dir, "moved.png");
      await raw.exportImage(png, false);
      assert.deepEqual(await pixel(png, 10, 10), [0, 0, 0, 0]);
      assert.deepEqual(await pixel(png, 30, 10), [255, 0, 0, 255]);
      dispatch({ type: "undo" });
      dispatch({ type: "deselectPixels" });
      dispatch({ type: "setTool", tool: "lasso" });
      dispatch({ type: "setLassoKind", kind: "polygonal" });
      pointer("down", 0, 0);
      pointer("up", 0, 0);
      pointer("down", 20, 0);
      pointer("up", 20, 0);
      pointer("down", 20, 20);
      pointer("up", 20, 20);
      state = dispatch({ type: "finishSelection" });
      assert.equal(state.hasPixelSelection, true);
      dispatch({ type: "deselectPixels" });
      dispatch({ type: "setTool", tool: "wand" });
      dispatch({
        type: "setWand",
        settings: { tolerance: 0, radius: 0, contiguous: true, sampleAllLayers: false },
      });
      const input: Command = {
        type: "pointer",
        samples: [
          {
            phase: "down",
            point: { x: 10, y: 10 },
            modifiers: { shift: false, alt: false, control: false, meta: false },
          },
        ],
      };
      assert.throws(() => dispatch(input), /dispatchAsync/);
      state = await worker(input);
      assert.deepEqual(state.pixelSelectionBounds, { x: 0, y: 0, width: 40, height: 40 });
      dispatch({ type: "deselectPixels" });
      dispatch({ type: "setTool", tool: "move" });
      dispatch({ type: "duplicate" });
      state = await worker({ type: "mergeLayers" });
      assert.equal(state.document.layers.length, 1);
      state = await worker({
        type: "resizeImage",
        options: { width: 160, height: 80, resolution: 300, sampling: "Nearest" },
      });
      assert.equal(state.document.width, 160);
      assert.equal(state.document.resolution, 300);
      const project = join(dir, "workflows.comp");
      await raw.save(project);
      const reopened = await native.openEditor(project);
      try {
        const saved: EditorState = JSON.parse(reopened.snapshot());
        assert.equal(saved.document.resolution, 300);
        assert.equal(saved.document.width, 160);
      } finally {
        reopened.close();
      }
      state = dispatch({ type: "undo" });
      assert.equal(state.document.width, 80);
      assert.equal(JSON.stringify(state).includes("pixels"), false);
    } finally {
      raw.close();
    }
  }));

test("actual addon retains guide and text drafts, typography, history and both project formats", async () =>
  temporary(async (dir) => {
    const raw = new native.NativeEditor(
      JSON.stringify({ kind: "new", name: "Typography", width: 400, height: 300 }),
    );
    const worker = async (command: Command): Promise<EditorState> =>
      JSON.parse(await raw.dispatchAsync(JSON.stringify(command)));
    const pointer = (phase: "down" | "up", x: number, y: number) =>
      worker({
        type: "pointer",
        samples: [
          {
            phase,
            point: { x, y },
            modifiers: { shift: false, alt: false, control: false, meta: false },
          },
        ],
      });
    try {
      await worker({
        type: "setViewport",
        viewport: { width: 400, height: 300, zoom: 1, pan: { x: 0, y: 0 } },
      });
      await worker({ type: "addGuide", axis: "vertical", position: 64 });
      let state = await worker({ type: "addGuide", axis: "horizontal", position: 40 });
      assert.equal(state.history.undoCount, 2);
      await worker({
        type: "setViewOptions",
        options: { ...state.viewOptions, rulers: true, grid: true, snapGrid: true },
      });
      await worker({ type: "setTool", tool: "text" });
      await pointer("down", 25, 70);
      state = await pointer("up", 25, 70);
      assert.equal(state.textEditing, true);
      const id = state.selection.ids[0]!;
      assert.throws(
        () =>
          raw.dispatch(
            JSON.stringify({ type: "updateText", patch: { text: "blocking" } } satisfies Command),
          ),
        /dispatchAsync/,
      );
      state = await worker({
        type: "updateText",
        patch: {
          text: "Native café\nTypography",
          fontName: "monospace",
          fontSize: 28,
          alignment: "center",
          tracking: 2,
          leading: 38,
          color: "#123456",
        },
      });
      assert.equal(state.history.undoCount, 2, "draft typing does not fill document history");
      assert.ok(state.currentText.width > 100);
      assert.equal(state.currentText.textLayout?.point, true);
      await assert.rejects(
        worker({ type: "setTextSelection", anchor: 11, head: 11 }),
        /Invalid text selection/,
      );
      state = await worker({ type: "setTextSelection", anchor: 0, head: 12 });
      assert.deepEqual(state.textSelection, { anchor: 0, head: 12 });
      state = await worker({ type: "setTextSelection", anchor: 0, head: 0 });
      const caretY = state.textCaret!.y;
      const firstCaret = state.textCaret!;
      state = await worker({ type: "setTextCaretVisible", visible: false });
      assert.equal(state.textCaretVisible, false);
      assert.equal(state.textEditing, true);
      assert.equal(state.history.undoCount, 2);
      state = await worker({
        type: "selectTextUnit",
        point: { x: firstCaret.x + 3, y: firstCaret.y + 14 },
        paragraph: false,
      });
      assert.deepEqual(state.textSelection, { anchor: 0, head: 6 });
      state = await worker({
        type: "selectTextUnit",
        point: { x: firstCaret.x + 3, y: firstCaret.y + 14 },
        paragraph: true,
      });
      assert.deepEqual(state.textSelection, { anchor: 0, head: 13 });
      await worker({ type: "setTextCaretVisible", visible: true });
      state = await worker({ type: "setTextSelection", anchor: 0, head: 0 });
      state = await worker({ type: "moveTextCaret", direction: "down", extend: true });
      assert.equal(state.textSelection.anchor, 0);
      assert.ok(state.textSelection.head > 0 && state.textCaret!.y > caretY);
      state = await worker({ type: "moveTextCaret", direction: "up", extend: false });
      assert.deepEqual(state.textSelection, { anchor: 0, head: 0 });
      state = await worker({ type: "commitText" });
      assert.equal(state.history.undoCount, 3);
      const committed = state.document.layers[0]!;
      await worker({ type: "editText", id });
      await worker({ type: "updateText", patch: { text: "Discard this draft" } });
      state = await worker({ type: "cancelText" });
      assert.deepEqual(state.document.layers[0], committed);
      const png = join(dir, "text.png");
      await raw.exportImage(png, false);
      const image = await loadImage(png);
      assert.equal(image.width, 400);
      for (const extension of ["picsie", "comp"]) {
        const path = join(dir, `layout.${extension}`);
        await raw.save(path);
        const reopened = await native.openEditor(path);
        try {
          const saved: EditorState = JSON.parse(reopened.snapshot());
          assert.deepEqual(saved.document.guides, state.document.guides);
          assert.deepEqual(saved.document.layers[0]?.content, committed.content);
          assert.deepEqual(saved.document.layers[0]?.textLayout, committed.textLayout);
          const output = join(dir, `reopened-${extension}.png`);
          await reopened.exportImage(output, false);
          assert.deepEqual(await readFile(output), await readFile(png));
          assert.equal(JSON.stringify(saved).includes('"data"'), false);
          assert.equal(JSON.stringify(saved).includes('"pixels"'), false);
        } finally {
          reopened.close();
        }
      }
      state = await worker({ type: "undo" });
      assert.equal(state.document.layers.length, 0);
      assert.equal(state.document.guides.length, 2);
    } finally {
      raw.close();
    }
  }));

test("actual addon preserves palette, transient blend previews, mask coverage and placement", async () =>
  temporary(async (dir) => {
    const editor = new native.NativeEditor(
      JSON.stringify({ kind: "new", name: "Polish", width: 100, height: 100 }),
    );
    const dispatch = (command: Command): EditorState =>
      JSON.parse(editor.dispatch(JSON.stringify(command)));
    const worker = async (command: Command): Promise<EditorState> =>
      JSON.parse(await editor.dispatchAsync(JSON.stringify(command)));
    try {
      dispatch({ type: "resizeViewport", width: 100, height: 100 });
      dispatch({ type: "setPaletteColor", color: "#ff0000", background: false });
      dispatch({ type: "setPaletteColor", color: "#00ff00", background: true });
      let state = dispatch({ type: "swapPaletteColors" });
      assert.equal(state.color, "#00ff00");
      assert.equal(state.backgroundColor, "#ff0000");
      dispatch({ type: "setTool", tool: "rectangle" });
      dispatch({
        type: "pointer",
        samples: [
          {
            phase: "down",
            point: { x: 0, y: 0 },
            modifiers: { shift: false, alt: false, control: false, meta: false },
          },
          {
            phase: "up",
            point: { x: 100, y: 100 },
            modifiers: { shift: false, alt: false, control: false, meta: false },
          },
        ],
      });
      state = JSON.parse(editor.snapshot());
      const id = state.selection.ids[0]!;
      const before = state.history.undoCount;
      state = dispatch({ type: "previewBlendMode", id, mode: "linear-burn" });
      assert.equal(state.document.layers[0]!.blend, "source-over");
      assert.equal(state.history.undoCount, before);
      assert.deepEqual(state.blendPreview, [id, "linear-burn"]);
      const png = join(dir, "committed.png");
      await editor.exportImage(png, false);
      assert.deepEqual(await pixel(png, 50, 50), [0, 255, 0, 255]);
      dispatch({ type: "previewBlendMode", id: null, mode: null });
      await worker({ type: "fillBackground" });
      await editor.exportImage(png, false);
      assert.deepEqual(await pixel(png, 50, 50), [255, 0, 0, 255]);
      dispatch({ type: "undo" });
      state = dispatch({ type: "cycleBlendMode", forward: false });
      assert.equal(state.document.layers[0]!.blend, "luminosity");
      dispatch({ type: "updateLayer", patch: { blend: "source-over" } });
      dispatch({ type: "setTool", tool: "marquee" });
      dispatch({
        type: "pointer",
        samples: [
          {
            phase: "down",
            point: { x: 30, y: 30 },
            modifiers: { shift: false, alt: false, control: false, meta: false },
          },
          {
            phase: "up",
            point: { x: 70, y: 70 },
            modifiers: { shift: false, alt: false, control: false, meta: false },
          },
        ],
      });
      state = dispatch({ type: "addMask", base: "reveal" });
      assert.equal(state.tool, "marquee");
      assert.equal(state.hasPixelSelection, false);
      state = await worker({ type: "loadThumbnailSelection", id, mask: true, mode: "replace" });
      assert.deepEqual(state.pixelSelectionBounds, { x: 30, y: 30, width: 40, height: 40 });
      dispatch({ type: "deselectPixels" });
      dispatch({ type: "toggleMaskLink" });
      await worker({
        type: "setMaskPlacement",
        placement: {
          sampling: "High",
          x: 10,
          y: 0,
          scaleX: 1,
          scaleY: 1,
          rotation: 0,
          flipX: false,
          flipY: false,
        },
      });
      await editor.exportImage(png, false);
      assert.equal((await pixel(png, 50, 50))[3], 0);
      assert.equal((await pixel(png, 35, 50))[3], 255);
      const serialized = editor.snapshot();
      assert.equal(serialized.includes('"pixels"'), false);
      assert.equal(serialized.includes('"strokes"'), false);
      const project = join(dir, "polish.picsie");
      await editor.save(project);
      const reopened = await native.openEditor(project);
      try {
        const second = join(dir, "reopened.png");
        await reopened.exportImage(second, false);
        assert.deepEqual(await readFile(second), await readFile(png));
      } finally {
        reopened.close();
      }
    } finally {
      editor.close();
    }
  }));

test("selection polish and persistent transform/distortion commands execute through the real addon", async () =>
  temporary(async (dir) => {
    const raw = new native.NativeEditor(
      JSON.stringify({ kind: "new", name: "Interactions", width: 100, height: 100 }),
    );
    const dispatch = (command: Command): EditorState =>
      JSON.parse(raw.dispatch(JSON.stringify(command)));
    const worker = async (command: Command): Promise<EditorState> =>
      JSON.parse(await raw.dispatchAsync(JSON.stringify(command)));
    try {
      dispatch({ type: "resizeViewport", width: 100, height: 100 });
      dispatch({ type: "setColor", color: "#ff0000" });
      dispatch({ type: "setTool", tool: "rectangle" });
      const modifiers = { shift: false, alt: false, control: false, meta: false };
      let state = dispatch({
        type: "pointer",
        samples: [
          { phase: "down", point: { x: 10, y: 10 }, modifiers },
          { phase: "up", point: { x: 60, y: 60 }, modifiers },
        ],
      });
      const before = state.history.undoCount;
      dispatch({ type: "setSelectionAntialiased", antialiased: false });
      dispatch({ type: "setTransformRatio", locked: true });
      state = await worker({ type: "setTransformField", field: "width", value: 100 });
      assert.equal(state.transformActive, true);
      assert.equal(state.transformScalePercent, 200);
      assert.equal(state.document.layers[0]!.scaleY, 2);
      state = dispatch({ type: "cancelTransform" });
      assert.equal(state.document.layers[0]!.scaleX, 1);
      assert.equal(state.history.undoCount, before);
      assert.throws(() => dispatch({ type: "beginDistort" }), /dispatchAsync/);
      state = await worker({
        type: "distortLayer",
        corners: [
          { x: 10, y: 10 },
          { x: 70, y: 10 },
          { x: 50, y: 60 },
          { x: 10, y: 60 },
        ],
      });
      assert.ok(state.imageDistortion);
      assert.equal(state.transformActive, true);
      state = await worker({ type: "commitTransform" });
      assert.equal(state.imageDistortion, null);
      assert.equal(state.history.undoCount, before + 1);
      const png = join(dir, "distorted.png");
      await raw.exportImage(png, false);
      assert.deepEqual(await pixel(png, 60, 15), [255, 0, 0, 255]);
      assert.equal((await pixel(png, 65, 55))[3], 0);
      state = await worker({ type: "selectLayerPixels" });
      assert.equal(state.hasPixelSelection, true);
      assert.equal(state.canModifySelection, true);
      assert.equal(state.selectionAntialiased, false);
      assert.equal(state.selectionEmpty, false);
    } finally {
      raw.close();
    }
  }));
