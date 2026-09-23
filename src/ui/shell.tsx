import {
  Dialog as NativeDialog,
  Menu,
  NativeNodeTag,
  Window,
  type NativeNode,
  type QuickGuiEvent,
} from "@quickgui/native";
import {
  Button,
  Dialog,
  Image,
  Select,
  Text,
  View,
  capturedPointerFromEvent,
  dropEventFromEvent,
  keyEventFromEvent,
  wheelEventFromEvent,
} from "@quickgui/solid";
import { For, Show, createEffect, createSignal } from "solid-js";
import { extname } from "node:path";
import { Editor } from "../engine/editor.ts";
import type { Layer } from "../engine/types.ts";
import { tools, blendLabels, blendModes } from "./editor-options.ts";
import {
  Action,
  Field,
  Section,
  TextEditor,
  colors,
  column,
  fieldRow,
  inputStyle,
  pickerAppearance,
  row,
} from "./controls.tsx";
import { Icon } from "./icons.tsx";
import { LayersPanel } from "./layers.tsx";
import { MaskPanel } from "./mask.tsx";
import { CanvasSizeDialog } from "./canvas-size.tsx";
import { registerCloseGuard } from "./lifecycle.ts";

export function Shell(props: {
  editor: Editor;
  path?: string;
  openWindow: (editor: Editor, path?: string) => void;
}) {
  const editor = props.editor;
  const window = Window.getCurrentWindow();
  const [revision, setRevision] = createSignal(0);
  const state = () => {
    revision();
    return editor;
  };
  const selected = () => state().selected;
  const textContent = () => {
    const content = selected()?.content;
    return content?.kind === "text" ? content : undefined;
  };
  const [frame, setFrame] = createSignal("");
  const [busy, setBusy] = createSignal(false);
  const [notice, setNotice] = createSignal(
    "Drag to move · Handles resize · Shift keeps proportions",
  );
  const [path, setPath] = createSignal(props.path);
  const [newOpen, setNewOpen] = createSignal(false);
  const [canvasSizeOpen, setCanvasSizeOpen] = createSignal(false);
  const showCanvasSize = () => {
    editor.finishGesture();
    setCanvasSizeOpen(true);
  };
  const [newName, setNewName] = createSignal("Untitled");
  const [newWidth, setNewWidth] = createSignal("1200");
  const [newHeight, setNewHeight] = createSignal("800");
  // Compositor's `selectionFeatherAmount`: the tool header applies this amount directly.
  const [featherAmount, setFeatherAmount] = createSignal(2);
  const commitFeather = (value: string) => {
    const parsed = Number(value.trim());
    setFeatherAmount(
      Number.isFinite(parsed) ? Math.min(250, Math.max(1, Math.round(parsed))) : 2,
    );
  };
  let stage: NativeNode | undefined;
  let shiftPressed = false;
  let altPressed = false;
  function trackModifiers(event: QuickGuiEvent) {
    const input = keyEventFromEvent(event);
    if (input) {
      shiftPressed = input.shift;
      altPressed = input.alt;
    }
  }
  let disposed = false,
    rendering = false,
    renderVersion = 0;
  let timer: ReturnType<typeof setTimeout> | undefined;

  const report = (error: unknown) =>
    setNotice(error instanceof Error ? error.message : String(error));
  const act = (callback: () => void) => {
    if (!busy()) {
      try {
        callback();
      } catch (error) {
        report(error);
        editor.notify();
      }
    }
  };
  const run = async (callback: () => Promise<void>) => {
    if (busy()) return;
    editor.finishGesture();
    setBusy(true);
    try {
      await callback();
    } catch (error) {
      report(error);
    } finally {
      if (!disposed) setBusy(false);
    }
  };
  async function render() {
    if (rendering || disposed) return;
    rendering = true;
    const version = renderVersion;
    try {
      const source = await editor.preview();
      // One render runs at a time: show progress during long drags, then render the latest state.
      if (!disposed) setFrame(source);
    } catch (error) {
      if (!disposed) report(error);
    } finally {
      rendering = false;
      if (!disposed && version !== renderVersion) schedule();
    }
  }
  function schedule() {
    if (timer) return;
    timer = setTimeout(() => {
      timer = undefined;
      void render();
    }, 24);
  }
  const changed = () => {
    setRevision((n) => n + 1);
    renderVersion++;
    schedule();
    window.setTitle(`${editor.document.name}${editor.history.dirty ? " •" : ""} — Picsie`);
    if (process.platform === "darwin") window.setDocumentEdited(editor.history.dirty);
  };

  async function save(as = false): Promise<boolean> {
    const doc = editor.document;
    let destination = as ? undefined : path();
    if (!destination) {
      const result = await NativeDialog.showSaveDialog(window, {
        title: "Save project",
        defaultPath: `${doc.name}.picsie`,
        filters: [
          { name: "Picsie project", extensions: ["picsie"] },
          { name: "Compositor package", extensions: ["comp"] },
        ],
      });
      if (result.canceled || !result.filePath) return false;
      destination = [".picsie", ".comp"].includes(extname(result.filePath).toLowerCase())
        ? result.filePath
        : `${result.filePath}.picsie`;
    }
    await editor.save(destination);
    setPath(destination);
    changed();
    const flattened = destination.endsWith(".comp") && doc.layers.some((layer) => ["shape", "gradient", "text"].includes(layer.content.kind));
    setNotice(flattened ? "Saved .comp package; live shapes, gradients, and text were rasterized" : `Saved ${doc.name}`);
    return true;
  }
  async function saveCompositor(): Promise<void> {
    const result = await NativeDialog.showSaveDialog(window, {
      title: "Save Compositor package",
      defaultPath: `${editor.document.name}.comp`,
      filters: [{ name: "Compositor package", extensions: ["comp"] }],
    });
    if (result.canceled || !result.filePath) return;
    const destination = result.filePath.toLowerCase().endsWith(".comp") ? result.filePath : `${result.filePath}.comp`;
    await editor.save(destination);
    setPath(destination);
    changed();
    const flattened = editor.document.layers.some((layer) => ["shape", "gradient", "text"].includes(layer.content.kind));
    setNotice(flattened ? "Saved .comp package; live shapes, gradients, and text were rasterized" : "Saved .comp package");
  }
  async function importPaths(paths: readonly string[]) {
    await editor.importImages(paths);
    if (paths.length) setNotice(`Imported ${paths.length} image${paths.length === 1 ? "" : "s"}`);
  }

  async function importDialog() {
    const result = await NativeDialog.showOpenDialog(window, {
      title: "Import images as layers",
      properties: ["openFile", "multiSelections"],
      filters: [{ name: "Images", extensions: ["png", "jpg", "jpeg", "webp"] }],
    });
    if (!result.canceled) await importPaths(result.filePaths);
  }
  async function open() {
    const result = await NativeDialog.showOpenDialog(window, {
      title: "Open project",
      properties: ["openFile"],
      filters: [
        { name: "Picsie and legacy projects", extensions: ["picsie", "electropic"] },
      ],
    });
    const file = result.filePaths[0];
    if (!result.canceled && file) props.openWindow(await Editor.open(file), file);
  }
  async function openCompositor() {
    const result = await NativeDialog.showOpenDialog(window, {
      title: "Open Compositor package",
      properties: ["openDirectory"],
    });
    const directory = result.filePaths[0];
    if (!result.canceled && directory) props.openWindow(await Editor.open(directory), directory);
  }
  async function exportImage(format: "png" | "jpeg") {
    const doc = editor.document;
    const extension = format === "png" ? "png" : "jpg";
    const result = await NativeDialog.showSaveDialog(window, {
      title: `Export ${format.toUpperCase()}`,
      defaultPath: `${doc.name}.${extension}`,
      filters: [
        { name: format.toUpperCase(), extensions: format === "png" ? ["png"] : ["jpg", "jpeg"] },
      ],
    });
    if (result.canceled || !result.filePath) return;
    const expected = format === "png" ? [".png"] : [".jpg", ".jpeg"];
    const destination = expected.includes(extname(result.filePath).toLowerCase())
      ? result.filePath
      : `${result.filePath}.${extension}`;
    await editor.export(destination, format);
    setNotice(`Exported ${doc.width} × ${doc.height} ${format.toUpperCase()}`);
  }
  const patch = (value: Partial<Layer>) => act(() => editor.updateLayer(value));
  const number = (value: string) => {
    if (!value.trim() || !Number.isFinite(Number(value))) throw new Error("Enter a finite number");
    return Number(value);
  };
  const numeric = (
    key: "x" | "y" | "rotation" | "opacity" | "brightness" | "saturation" | "blur",
    value: string,
    divisor = 1,
  ) => act(() => editor.updateLayer({ [key]: number(value) / divisor }));

  async function confirmClose(): Promise<boolean> {
    if (busy()) return false;
    editor.finishGesture();
    if (!editor.history.dirty) return true;
    setBusy(true);
    try {
      const choice = await NativeDialog.showAlertDialog(window, {
        level: "warning",
        message: `Save changes to ${editor.document.name}?`,
        detail: "Your project has unsaved edits.",
        buttons: [
          { label: "Save", role: "default" },
          { label: "Cancel", role: "cancel" },
          { label: "Discard", role: "other" },
        ],
      });
      return choice === 2 || (choice === 0 && (await save()));
    } catch (error) {
      report(error);
      return false;
    } finally {
      if (!disposed) setBusy(false);
    }
  }

  function installMenu() {
    // QuickGUI 0.1.6 does not implement application menus on Linux.
    if (process.platform === "linux") return;
    Menu.setApplicationMenu([
      {
        label: "File",
        items: [
          { label: "New…", accelerator: "CmdOrCtrl+N", click: () => act(() => setNewOpen(true)) },
          { label: "Open Project…", accelerator: "CmdOrCtrl+O", click: () => void run(open) },
          { label: "Open Compositor Package…", click: () => void run(openCompositor) },
          {
            label: "Import Image…",
            accelerator: "CmdOrCtrl+Shift+O",
            click: () => void run(importDialog),
          },
          { type: "separator" },
          {
            label: "Save Project",
            accelerator: "CmdOrCtrl+S",
            click: () =>
              void run(async () => {
                await save();
              }),
          },
          {
            label: "Save Project As…",
            accelerator: "CmdOrCtrl+Shift+S",
            click: () =>
              void run(async () => {
                await save(true);
              }),
          },
          { label: "Save Compositor Package…", click: () => void run(saveCompositor) },
          {
            label: "Export PNG…",
            accelerator: "CmdOrCtrl+Alt+S",
            click: () => void run(() => exportImage("png")),
          },
          { label: "Export JPEG…", click: () => void run(() => exportImage("jpeg")) },
          { type: "role", role: "close-window", label: "Close Window", accelerator: "CmdOrCtrl+W" },
        ],
      },
      {
        label: "Edit",
        items: [
          {
            label: "Undo Canvas Edit",
            accelerator: "CmdOrCtrl+Z",
            click: () => act(() => editor.undo()),
          },
          {
            label: "Redo Canvas Edit",
            accelerator: "CmdOrCtrl+Shift+Z",
            click: () => act(() => editor.redo()),
          },
          { type: "separator" },
          { type: "role", role: "cut", label: "Cut", accelerator: "CmdOrCtrl+X" },
          { type: "role", role: "copy", label: "Copy", accelerator: "CmdOrCtrl+C" },
          { type: "role", role: "paste", label: "Paste", accelerator: "CmdOrCtrl+V" },
          { type: "role", role: "select-all", label: "Select All", accelerator: "CmdOrCtrl+A" },
        ],
      },
      {
        label: "Image",
        items: [
          {
            label: "Canvas Size…",
            accelerator: "CmdOrCtrl+Alt+C",
            click: () => act(showCanvasSize),
          },
        ],
      },
      {
        label: "Layer",
        items: [
          {
            label: "New Paint Layer",
            accelerator: "CmdOrCtrl+Shift+N",
            click: () => act(() => editor.addPaintLayer()),
          },
          { label: "New Gradient Layer", click: () => act(() => editor.addGradient()) },
          {
            label: "Duplicate Layer",
            accelerator: "CmdOrCtrl+J",
            click: () => act(() => editor.duplicate()),
          },
          {
            label: "Raise Layer",
            accelerator: "CmdOrCtrl+]",
            click: () => act(() => editor.reorder(1)),
          },
          {
            label: "Lower Layer",
            accelerator: "CmdOrCtrl+[",
            click: () => act(() => editor.reorder(-1)),
          },
        ],
      },
      {
        label: "View",
        items: [
          { label: "Fit Canvas", accelerator: "CmdOrCtrl+0", click: () => act(() => editor.fit()) },
          {
            label: "Actual Size",
            accelerator: "CmdOrCtrl+1",
            click: () => act(() => editor.zoom(1)),
          },
          {
            label: "Zoom In",
            accelerator: "CmdOrCtrl+=",
            click: () => act(() => editor.zoom(editor.viewport.zoom * 1.25)),
          },
          {
            label: "Zoom Out",
            accelerator: "CmdOrCtrl+-",
            click: () => act(() => editor.zoom(editor.viewport.zoom / 1.25)),
          },
        ],
      },
    ]);
  }

  function keyDown(event: QuickGuiEvent) {
    if (busy() || newOpen() || canvasSizeOpen() || event.target.tag === NativeNodeTag.Input) return;
    const key = keyEventFromEvent(event);
    if (key) {
      shiftPressed = key.shift;
      altPressed = key.alt;
    }
    if (key && (key.control || key.meta) && key.key.toLowerCase() === "a") {
      if (editor.tool === "marquee" || editor.tool === "lasso") editor.selectAllPixels();
      else editor.selectAll();
      return;
    }
    if (key && process.platform === "linux" && (key.control || key.meta)) {
      const letter = key.key.toLowerCase();
      if (letter === "z") act(() => (key.shift ? editor.redo() : editor.undo()));
      else if (letter === "j") act(() => editor.duplicate());
      else if (letter === "c" && key.alt) act(showCanvasSize);
      else if (letter === "n") act(() => (key.shift ? editor.addPaintLayer() : setNewOpen(true)));
      else if (letter === "o") void run(key.shift ? importDialog : open);
      else if (letter === "s")
        void run(async () => {
          if (key.alt) await exportImage(key.shift ? "jpeg" : "png");
          else await save(key.shift);
        });
      else if (letter === "0") editor.fit();
      else if (letter === "1") editor.zoom(1);
      else if (letter === "=" || letter === "+") editor.zoom(editor.viewport.zoom * 1.25);
      else if (letter === "-") editor.zoom(editor.viewport.zoom / 1.25);
      else if (letter === "[") editor.reorder(-1);
      else if (letter === "]") editor.reorder(1);
      return;
    }
    if (!key || key.meta || key.control || key.alt) return;
    const tool = tools.find((tool) => tool.key.toLowerCase() === key.key.toLowerCase());
    if (tool) {
      editor.setTool(tool.id);
      return;
    }
    if (key.key === "Escape") {
      if (editor.tool === "crop") editor.cancelCrop();
      else if (editor.tool === "marquee" || editor.tool === "lasso") editor.deselectPixels();
      else editor.cancelGesture();
    }
    if (key.key === "Enter" && editor.tool === "crop") editor.commitCrop();
    if (key.key.toLowerCase() === "x" && editor.paintTarget === "mask")
      editor.setMaskMode(editor.maskMode === "hide" ? "reveal" : "hide");
    if (key.key === "Delete" || key.key === "Backspace") {
      if (editor.pixelSelectionBounds) editor.clearSelectedPixels();
      else editor.remove();
    }
    if (key.key === "[") {
      editor.brushSize = Math.max(1, editor.brushSize - 5);
      editor.notify();
    }
    if (key.key === "]") {
      editor.brushSize = Math.min(1000, editor.brushSize + 5);
      editor.notify();
    }
    const layer = editor.selected,
      step = key.shift ? 10 : 1;
    if (layer && key.key.startsWith("Arrow"))
      editor.nudge({
        x: key.key === "ArrowRight" ? step : key.key === "ArrowLeft" ? -step : 0,
        y: key.key === "ArrowDown" ? step : key.key === "ArrowUp" ? -step : 0,
      });
  }
  function pointer(event: QuickGuiEvent) {
    if (busy()) return;
    const pointer = capturedPointerFromEvent(event);
    if (pointer.button !== "left") return;
    stage?.focus();
    if (editor.tool === "eyedropper" && pointer.phase === "down") {
      void run(() => editor.sampleColor(pointer.localPosition));
    } else
      act(() =>
        editor.pointer(pointer.phase, pointer.localPosition, {
          shift: shiftPressed,
          alt: altPressed,
        }),
      );
  }

  createEffect(
    () => 0,
    () => {
      const unsubscribe = editor.subscribe(changed);
      const offResize = window.on("resize", ({ size }) =>
        editor.resizeViewport(size.width - 344, size.height - 126),
      );
      const offFocus = window.on("focus", () => {
        shiftPressed = false;
        altPressed = false;
        installMenu();
      });
      const offBlur = window.on("blur", () => {
        shiftPressed = false;
        altPressed = false;
      });
      const offClose = window.onCloseRequested(() => {
        void confirmClose().then((allowed) => {
          if (allowed) window.close();
        });
      });
      const unregisterGuard = registerCloseGuard(window, confirmClose, () => editor.document);
      installMenu();
      // Solid flushes mount effects inside Window's constructor, before native registration.
      // Defer window APIs until that constructor has completed.
      queueMicrotask(() => {
        if (disposed) return;
        void window
          .whenReady()
          .then(() => window.getState())
          .then(({ viewportSize }) => {
            if (disposed) return;
            editor.resizeViewport(viewportSize.width - 344, viewportSize.height - 126);
            editor.fit();
          })
          .catch(report);
      });
      return () => {
        disposed = true;
        if (timer) clearTimeout(timer);
        unsubscribe();
        offResize();
        offFocus();
        offBlur();
        offClose();
        unregisterGuard();
        editor.close();
      };
    },
  );

  return (
    <View
      onKeyDown={trackModifiers}
      onKeyUp={trackModifiers}
      onMouseDown={trackModifiers}
      style={{
        ...column,
        gap: 0,
        width: "100%",
        height: "100%",
        bg: colors.bg,
        color: colors.text,
        fontFamily: "sans-serif",
        fontSize: 12,
      }}
    >
      <View
        style={{
          ...row,
          height: 52,
          flexShrink: 0,
          paddingLeft: 18,
          paddingRight: 18,
          borderBottomWidth: 1,
          borderColor: colors.line,
        }}
      >
        <Icon name="image" size={20} color={colors.accent} />
        <Text style={{ fontSize: 14, fontWeight: 600, letterSpacing: 0.6 }}>picsie</Text>
        <Text style={{ color: colors.muted, fontSize: 11, marginLeft: 12 }}>IMAGE EDITOR</Text>
        <View style={{ flexGrow: 1 }} />
        <Action label="New" onClick={() => setNewOpen(true)} disabled={busy()} />
        <Action label="Open" onClick={() => void run(open)} disabled={busy()} />
        <Action label="Open .comp" onClick={() => void run(openCompositor)} disabled={busy()} />
        <Action label="Import image" onClick={() => void run(importDialog)} disabled={busy()} />
        <Action
          label="Save"
          onClick={() =>
            void run(async () => {
              await save();
            })
          }
          disabled={busy()}
        />
        <Action label="Save .comp" onClick={() => void run(saveCompositor)} disabled={busy()} />
        <Action
          label="Export PNG"
          icon="export"
          primary
          onClick={() => void run(() => exportImage("png"))}
          disabled={busy()}
        />
      </View>
      <View
        style={{
          ...row,
          height: 46,
          flexShrink: 0,
          paddingLeft: 18,
          paddingRight: 18,
          borderBottomWidth: 1,
          borderColor: colors.line,
        }}
      >
        <Text style={{ fontSize: 12, color: colors.accent, width: 98 }}>
          {tools.find((t) => t.id === state().tool)?.label}
        </Text>
        <Show
          when={state().tool === "brush" || state().tool === "eraser"}
          fallback={
            <Show
              when={state().tool === "crop"}
              fallback={
                <Show when={state().tool === "marquee" || state().tool === "lasso"} fallback={<Text style={{ color: colors.muted, fontSize: 11 }}>V move · B brush · M marquee · L lasso · C crop</Text>}>
                  <View style={{ ...row, gap: 5 }}>
                    <Show when={state().tool === "marquee"}>
                      <Action label="Rectangle" active={state().marqueeKind === "rectangle"} onClick={() => act(() => editor.setMarqueeKind("rectangle"))} />
                      <Action label="Ellipse" active={state().marqueeKind === "ellipse"} onClick={() => act(() => editor.setMarqueeKind("ellipse"))} />
                    </Show>
                    <For each={[{ id: "replace", label: "New" }, { id: "add", label: "Add" }, { id: "subtract", label: "Subtract" }] as const}>
                      {(mode) => <Action label={mode.label} active={state().selectionMode === mode.id} onClick={() => act(() => editor.setSelectionMode(mode.id))} />}
                    </For>
                    <View style={{ ...row, gap: 5, width: 132 }}>
                      <Field
                        inline
                        label="Feather px"
                        value={featherAmount()}
                        onCommit={commitFeather}
                      />
                    </View>
                    <Action
                      label="Feather"
                      tooltip="Fade the edge of the selection by this many pixels"
                      disabled={!state().pixelSelectionBounds || busy()}
                      onClick={() => act(() => editor.featherSelection(featherAmount()))}
                    />
                    <Action label="Clear pixels" disabled={!state().pixelSelectionBounds || !state().selected} onClick={() => act(() => editor.clearSelectedPixels())} />
                    <Action label="Deselect" disabled={!state().pixelSelectionBounds} onClick={() => act(() => editor.deselectPixels())} />
                  </View>
                </Show>
              }
            >
              <View style={{ ...row, gap: 4 }}>
                <For each={[
                  { id: "free", label: "Free" },
                  { id: "original", label: "Original" },
                  { id: "square", label: "1:1" },
                  { id: "fourThree", label: "4:3" },
                  { id: "sixteenNine", label: "16:9" },
                ] as const}>
                  {(ratio) => <Action label={ratio.label} active={state().cropRatio === ratio.id} onClick={() => act(() => editor.setCropRatio(ratio.id))} />}
                </For>
                <Action label="Apply" primary onClick={() => act(() => editor.commitCrop())} />
                <Action label="Cancel" onClick={() => act(() => editor.cancelCrop())} />
              </View>
            </Show>
          }
        >
          <View style={{ ...row, width: 245 }}>
            <Field
              inline
              label="Size (px)"
              value={state().brushSize}
              onCommit={(v) =>
                act(() => {
                  editor.brushSize = Math.max(1, Math.min(1000, number(v)));
                  editor.notify();
                })
              }
            />
            <Field
              inline
              label="Flow (%)"
              value={Math.round(state().brushOpacity * 100)}
              onCommit={(v) =>
                act(() => {
                  editor.brushOpacity = Math.max(0, Math.min(1, number(v) / 100));
                  editor.notify();
                })
              }
            />
          </View>
        </Show>
        <Show when={state().paintTarget === "mask"}>
          <Text style={{ color: "#62deca", fontSize: 11 }}>Mask</Text>
          <Action
            label="Hide"
            active={state().maskMode === "hide"}
            onClick={() => editor.setMaskMode("hide")}
          />
          <Action
            label="Reveal"
            active={state().maskMode === "reveal"}
            onClick={() => editor.setMaskMode("reveal")}
          />
        </Show>
        <View style={{ flexGrow: 1 }} />
        <Action
          label="Undo"
          icon="undo"
          iconOnly
          tooltip={`Undo ${state().history.undoLabel}`}
          disabled={!state().history.canUndo || busy()}
          onClick={() => act(() => editor.undo())}
        />
        <Action
          label="Redo"
          icon="redo"
          iconOnly
          tooltip={`Redo ${state().history.redoLabel}`}
          disabled={!state().history.canRedo || busy()}
          onClick={() => act(() => editor.redo())}
        />
        <Text style={{ fontSize: 11, color: colors.muted, marginLeft: 10 }}>
          {state().document.name}
          {state().history.dirty ? " •" : ""}
        </Text>
      </View>
      <View style={{ ...row, alignItems: "stretch", gap: 0, flexGrow: 1, minHeight: 0 }}>
        <View
          style={{
            ...column,
            gap: 7,
            width: 64,
            flexShrink: 0,
            padding: 10,
            alignItems: "center",
            borderRightWidth: 1,
            borderColor: colors.line,
          }}
        >
          <For each={tools}>
            {(tool) => (
              <Action
                label={tool.label}
                icon={tool.id}
                iconOnly
                tooltip={`${tool.label} (${tool.key})`}
                active={state().tool === tool.id}
                disabled={busy()}
                style={{ width: 42, height: 37, padding: 0 }}
                onClick={() =>
                  act(() => {
                    editor.setTool(tool.id);
                    stage?.focus();
                  })
                }
              />
            )}
          </For>
          <View style={{ height: 10 }} />
          <View
            tooltip="Foreground color"
            style={{
              height: 32,
              width: 32,
              borderRadius: 6,
              bg: state().color,
              borderWidth: 2,
              borderColor: "#edf0fc",
            }}
          />
          <View style={{ flexGrow: 1 }} />
          <Text style={{ color: colors.muted, fontSize: 9, textAlign: "center" }}>RGB</Text>
        </View>
        <View
          ref={(node) => {
            stage = node;
          }}
          tabIndex={0}
          ariaLabel="Image canvas"
          onKeyDown={keyDown}
          onPointer={pointer}
          onMouseDown={trackModifiers}
          onMouseMove={trackModifiers}
          onWheel={(event) => {
            if (busy()) return;
            const wheel = wheelEventFromEvent(event);
            if (!wheel) return;
            if (wheel.control || wheel.meta)
              editor.zoom(editor.viewport.zoom * Math.exp(-wheel.deltaY * 0.01));
            else {
              editor.pan({ x: -wheel.deltaX, y: -wheel.deltaY });
            }
          }}
          dropKinds="files"
          onFilesDropped={(event) => {
            const files = dropEventFromEvent(event)?.paths;
            if (files?.length) void run(() => importPaths(files));
          }}
          style={{
            flexGrow: 1,
            minWidth: 0,
            overflow: "hidden",
            bg: "#15171c",
            cursor:
              state().tool === "hand" ? "grab" : state().tool === "move" ? "default" : "crosshair",
          }}
        >
          <Image
            source={frame()}
            fit="fill"
            style={{ width: state().viewport.width, height: state().viewport.height }}
          />
        </View>
        <View
          style={{
            ...column,
            gap: 0,
            width: 280,
            flexShrink: 0,
            bg: colors.panel,
            borderLeftWidth: 1,
            borderColor: colors.line,
            overflowY: "auto",
          }}
        >
          <Section title="Color">
            <View style={fieldRow}>
              <View style={{ width: 33, height: 33, borderRadius: 6, bg: state().color }} />
              <Field
                label="Foreground"
                value={state().color}
                onCommit={(v) => act(() => editor.setColor(v))}
              />
            </View>
            <View style={{ ...row, gap: 7 }}>
              <For each={["#fff5e8", "#a5b4fc", "#6587ff", "#f6a484", "#e86c86", "#18213b"]}>
                {(color) => (
                  <Button
                    ariaLabel={`Use ${color}`}
                    tooltip={color}
                    onClick={() => act(() => editor.setColor(color))}
                    style={{
                      width: 29,
                      height: 20,
                      bg: color,
                      borderRadius: 3,
                      borderWidth: state().color === color ? 2 : 0,
                      borderColor: "#ffffff",
                    }}
                  />
                )}
              </For>
            </View>
          </Section>
          <LayersPanel editor={state} busy={busy()} act={act} />
          <Show when={state().selectedIds.length === 1 ? selected() : undefined}>
            {(layer) => (
              <>
                <Section title="Properties">
                  <Field
                    wide
                    label="Name"
                    value={layer().name}
                    disabled={layer().locked || busy()}
                    onCommit={(name) => patch({ name })}
                  />
                  <View style={fieldRow}>
                    <View style={{ ...column, gap: 5, width: 164, flexShrink: 0 }}>
                      <Text style={{ color: colors.muted, fontSize: 10, lineHeight: "14px" }}>
                        Blend mode
                      </Text>
                      <Select
                        ariaLabel="Blend mode"
                        appearance={pickerAppearance}
                        value={layer().blend}
                        items={blendModes.map((value) => ({ value, label: blendLabels[value] }))}
                        onValueChange={(v) => {
                          const blend = blendModes.find((mode) => mode === v);
                          if (blend) patch({ blend });
                        }}
                        disabled={layer().locked || busy() || layer().content.kind === "group"}
                        style={{
                          ...inputStyle,
                          ...row,
                          width: "100%",
                          flexGrow: 0,
                          justifyContent: "space-between",
                        }}
                      >
                        <Select.Value>
                          <Text>{blendLabels[layer().blend]}</Text>
                        </Select.Value>
                        <Select.Icon>
                          <Icon name="chevron" size={14} />
                        </Select.Icon>
                        <Select.Positioner side="top" align="start" sideOffset={4} />
                      </Select>
                    </View>
                    <Field
                      label="Opacity %"
                      value={Math.round(layer().opacity * 100)}
                      disabled={layer().locked || busy()}
                      onCommit={(v) => numeric("opacity", v, 100)}
                    />
                  </View>
                  <Action
                    label={layer().locked ? "Unlock layer" : "Lock layer"}
                    active={layer().locked}
                    onClick={() => patch({ locked: !layer().locked })}
                  />
                </Section>
                <Show when={layer().content.kind !== "group"}>
                <MaskPanel editor={state} busy={busy()} act={act} />
                <Section title="Transform">
                  <View style={fieldRow}>
                    <Field
                      label="X"
                      value={Math.round(layer().x)}
                      onCommit={(v) => numeric("x", v)}
                      disabled={layer().locked}
                    />
                    <Field
                      label="Y"
                      value={Math.round(layer().y)}
                      onCommit={(v) => numeric("y", v)}
                      disabled={layer().locked}
                    />
                  </View>
                  <View style={fieldRow}>
                    <Field
                      label="Width"
                      value={Math.round(layer().width * layer().scaleX)}
                      onCommit={(v) =>
                        act(() => editor.updateLayer({ scaleX: number(v) / layer().width }))
                      }
                      disabled={layer().locked}
                    />
                    <Field
                      label="Height"
                      value={Math.round(layer().height * layer().scaleY)}
                      onCommit={(v) =>
                        act(() => editor.updateLayer({ scaleY: number(v) / layer().height }))
                      }
                      disabled={layer().locked}
                    />
                  </View>
                  <View style={fieldRow}>
                    <Field
                      label="Rotation °"
                      value={Math.round(layer().rotation * 10) / 10}
                      onCommit={(v) => numeric("rotation", v)}
                      disabled={layer().locked}
                    />
                    <Action label="Flip X" onClick={() => patch({ flipX: !layer().flipX })} />
                    <Action label="Flip Y" onClick={() => patch({ flipY: !layer().flipY })} />
                  </View>
                </Section>
                <Show when={layer().content.kind === "text"}>
                  <Section title="Text">
                    <TextEditor
                      value={textContent()?.text ?? ""}
                      disabled={layer().locked || busy()}
                      onCommit={(text) => {
                        const content = editor.selected?.content;
                        if (content?.kind === "text") patch({ content: { ...content, text } });
                      }}
                    />
                    <Field
                      wide
                      label="Font size"
                      value={textContent()?.fontSize ?? 64}
                      onCommit={(v) =>
                        act(() => {
                          const content = editor.selected?.content;
                          if (content?.kind === "text")
                            editor.updateLayer({ content: { ...content, fontSize: number(v) } });
                        })
                      }
                      disabled={layer().locked}
                    />
                    <Select
                      ariaLabel="Font family"
                      appearance={pickerAppearance}
                      value={textContent()?.fontFamily ?? "sans-serif"}
                      items={{ "sans-serif": "Sans serif", serif: "Serif", monospace: "Monospace" }}
                      onValueChange={(font) => {
                        const content = editor.selected?.content;
                        if (
                          content?.kind === "text" &&
                          (font === "sans-serif" || font === "serif" || font === "monospace")
                        )
                          patch({ content: { ...content, fontFamily: font } });
                      }}
                      style={{ ...inputStyle, ...row, justifyContent: "space-between" }}
                    >
                      <Select.Value>
                        <Text>{textContent()?.fontFamily ?? "sans-serif"}</Text>
                      </Select.Value>
                      <Select.Icon>
                        <Icon name="chevron" size={14} />
                      </Select.Icon>
                      <Select.Positioner side="top" align="start" sideOffset={4} />
                    </Select>
                  </Section>
                </Show>
                <Show
                  when={
                    layer().content.kind === "text" ||
                    layer().content.kind === "shape" ||
                    layer().content.kind === "gradient"
                  }
                >
                  <Section title="Fill">
                    <Action
                      label="Use foreground color"
                      onClick={() => {
                        const content = editor.selected?.content;
                        if (content?.kind === "shape" || content?.kind === "text")
                          patch({ content: { ...content, color: editor.color } });
                        if (content?.kind === "gradient")
                          patch({ content: { ...content, from: editor.color } });
                      }}
                    />
                    <Show when={layer().content.kind === "gradient"}>
                      <Action
                        label="Use foreground for end color"
                        onClick={() => {
                          const content = editor.selected?.content;
                          if (content?.kind === "gradient")
                            patch({ content: { ...content, to: editor.color } });
                        }}
                      />
                    </Show>
                  </Section>
                </Show>
                <Section title="Adjustments">
                  <View style={{ ...row }}>
                    <Field
                      label="Brightness %"
                      value={Math.round(layer().brightness * 100)}
                      onCommit={(v) => numeric("brightness", v, 100)}
                      disabled={layer().locked}
                    />
                    <Field
                      label="Saturation %"
                      value={Math.round(layer().saturation * 100)}
                      onCommit={(v) => numeric("saturation", v, 100)}
                      disabled={layer().locked}
                    />
                  </View>
                  <Field
                    wide
                    label="Gaussian blur (px)"
                    value={layer().blur}
                    onCommit={(v) => numeric("blur", v)}
                    disabled={layer().locked}
                  />
                </Section>
                </Show>
              </>
            )}
          </Show>
        </View>
      </View>
      <View
        style={{
          ...row,
          height: 28,
          flexShrink: 0,
          paddingLeft: 14,
          paddingRight: 14,
          borderTopWidth: 1,
          borderColor: colors.line,
        }}
      >
        <Action
          label={`${state().document.width} × ${state().document.height} px`}
          tooltip="Canvas Size (Ctrl/⌘+Alt+C)"
          disabled={busy()}
          style={{ height: 22, paddingLeft: 6, paddingRight: 6 }}
          onClick={showCanvasSize}
        />
        <Text
          style={{
            fontSize: 10,
            color: busy() ? colors.accent : colors.muted,
            flexGrow: 1,
            marginLeft: 12,
            textOverflow: "ellipsis",
          }}
        >
          {busy() ? "Working…" : notice()}
        </Text>
        <Action
          label="Zoom out"
          icon="minus"
          iconOnly
          style={{ height: 22, width: 26 }}
          onClick={() => editor.zoom(editor.viewport.zoom / 1.25)}
        />
        <Text style={{ fontSize: 10, width: 35, textAlign: "center" }}>
          {Math.round(state().viewport.zoom * 100)}%
        </Text>
        <Action
          label="Zoom in"
          icon="plus"
          iconOnly
          style={{ height: 22, width: 26 }}
          onClick={() => editor.zoom(editor.viewport.zoom * 1.25)}
        />
        <Action label="Fit" style={{ height: 22 }} onClick={() => editor.fit()} />
      </View>
      <Show when={canvasSizeOpen()}>
        <CanvasSizeDialog
          document={editor.document}
          foreground={editor.color}
          onClose={() => setCanvasSizeOpen(false)}
          onApply={async (options) => {
            setBusy(true);
            try {
              await editor.resizeCanvas(options);
              setNotice(
                `Canvas resized to ${editor.document.width} × ${editor.document.height} pixels`,
              );
            } finally {
              if (!disposed) setBusy(false);
            }
          }}
        />
      </Show>
      <Dialog.Root open={newOpen()} onOpenChange={setNewOpen}>
        <Dialog.Portal style={{ display: "flex", alignItems: "center", justifyContent: "center" }}>
          <Dialog.Backdrop
            style={{ position: "absolute", top: 0, right: 0, bottom: 0, left: 0, bg: "#00000099" }}
          />
          <Dialog.Popup
            style={{
              ...column,
              width: 360,
              padding: 24,
              borderRadius: 12,
              bg: colors.panel,
              color: colors.text,
              borderWidth: 1,
              borderColor: colors.line,
            }}
          >
            <Dialog.Title style={{ fontSize: 20, fontWeight: 600 }}>New canvas</Dialog.Title>
            <Dialog.Description style={{ fontSize: 12, color: colors.muted }}>
              Start with a transparent canvas.
            </Dialog.Description>
            <Field wide label="Project name" value={newName()} onCommit={setNewName} />
            <View style={row}>
              <Field label="Width (px)" value={newWidth()} onCommit={setNewWidth} />
              <Field label="Height (px)" value={newHeight()} onCommit={setNewHeight} />
            </View>
            <View style={{ ...row, justifyContent: "flex-end", marginTop: 8 }}>
              <Action label="Cancel" onClick={() => setNewOpen(false)} />
              <Action
                label="Create"
                primary
                onClick={() =>
                  act(() => {
                    const doc = new Editor({
                      kind: "new",
                      name: newName(),
                      width: number(newWidth()),
                      height: number(newHeight()),
                    });
                    props.openWindow(doc);
                    setNewOpen(false);
                  })
                }
              />
            </View>
          </Dialog.Popup>
        </Dialog.Portal>
      </Dialog.Root>
    </View>
  );
}
