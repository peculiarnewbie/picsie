import test from "node:test";
import assert from "node:assert/strict";
import { mkdtemp, readFile, rm, stat, readdir, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { createRequire } from "node:module";
import { createCanvas, loadImage } from "@napi-rs/canvas";
import { Editor } from "../src/engine/editor.ts";
import { CanvasSizeDraft } from "../src/ui/canvas-size-draft.ts";
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
        assert.equal(reopened.document.layers.find((layer) => layer.id === child)?.parentId, folder);
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
