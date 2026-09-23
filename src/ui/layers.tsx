import {
  Button,
  Text,
  View,
  capturedPointerFromEvent,
  mouseEventFromEvent,
  keyEventFromEvent,
} from "@quickgui/solid";
import { For, Show, createSignal } from "solid-js";
import type { QuickGuiEvent } from "@quickgui/native";
import type { Editor, SelectionMode } from "../engine/editor.ts";
import { Action, Section, colors, column, row } from "./controls.tsx";
import { Icon } from "./icons.tsx";

/** Fixed-height rows keep insertion markers and drag destinations in the same coordinate system. */
export function LayersPanel(props: {
  editor: () => Editor;
  busy: boolean;
  act: (callback: () => void) => void;
}) {
  const [drop, setDrop] = createSignal<{ id: string; side: "above" | "below" | "into" }>();
  let drag: { startY: number; index: number; ids: string[]; moved: boolean } | undefined;
  // 40px rows with a 3px gap; fixed heights keep every drop in one coordinate system.
  const ROW = 43;
  const state = props.editor;
  function reorder(id: string, event: QuickGuiEvent) {
    if (props.busy) return;
    const pointer = capturedPointerFromEvent(event);
    if (pointer.button !== "left") return;
    if (pointer.phase === "down") {
      if (!state().isSelected(id)) state().select(id);
      if (!state().selectedLayers.some((layer) => !layer.locked)) return;
      const ids = state().layerRows.map((row) => row.id);
      drag = {
        startY: ids.indexOf(id) * ROW + pointer.localPosition.y,
        index: ids.indexOf(id),
        ids,
        moved: false,
      };
    } else if (pointer.phase === "cancel") {
      drag = undefined;
      setDrop(undefined);
    } else if (drag) {
      // Displacement from the press point, not `delta`: the release event's delta is not the
      // drag's total. Fixed row heights keep every drop in one coordinate system.
      const dx = pointer.localPosition.x - pointer.localOrigin.x;
      const dy = pointer.localPosition.y - pointer.localOrigin.y;
      drag.moved ||= Math.abs(dy) > 4;
      const rows = state().layerRows;
      const y = drag.startY + dy;
      const boundary = Math.max(0, Math.min(rows.length, Math.round(y / ROW)));
      const hovered = Math.max(0, Math.min(rows.length - 1, Math.floor(y / ROW)));
      const target = rows[Math.min(boundary, rows.length - 1)]!;
      const within = y - hovered * ROW;
      const folder = rows[hovered]!;
      const layer = state().document.layers.find((layer) => layer.id === folder.id);
      // Upstream drops onto a folder row to file layers inside it; a dragged folder can
      // never be filed inside itself or its own subtree.
      const into =
        layer?.content.kind === "group" &&
        within > 10 &&
        within < 33 &&
        !state().isSelected(folder.id) &&
        !state().selectedLayers.some((selected) => selected.id === folder.id);
      const movable = drag.moved && Math.abs(dx) < 280;
      const destination = into
        ? { id: folder.id, side: "into" as const }
        : {
            id: target.id,
            side: boundary === rows.length ? ("below" as const) : ("above" as const),
          };
      const valid = movable && (into || !state().isSelected(destination.id));
      setDrop(valid ? destination : undefined);
      if (pointer.phase === "up") {
        if (valid)
          props.act(() =>
            destination.side === "into"
              ? state().moveToGroup(destination.id)
              : state().reorderTo(destination.id, destination.side),
          );
        drag = undefined;
        setDrop(undefined);
      }
    }
  }
  return (
    <Section title="Layers">
      <View style={{ ...row, gap: 5 }}>
        <Action
          label="Paint"
          icon="plus"
          disabled={props.busy}
          onClick={() => props.act(() => state().addPaintLayer())}
        />
        <Action
          label="Gradient"
          disabled={props.busy}
          onClick={() => props.act(() => state().addGradient())}
        />
        <Action label="Folder" disabled={props.busy} onClick={() => props.act(() => state().addGroup())} />
        <Text style={{ color: colors.muted, fontSize: 11 }}>{state().document.layers.length}</Text>
      </View>
      <Text style={{ color: colors.muted, fontSize: 10, lineHeight: "14px" }}>
        Shift: range · Ctrl/⌘: toggle · Drag rows: reorder · Drop on a folder: file inside
      </Text>
      <View style={{ ...column, gap: 3, maxHeight: 255, overflowY: "auto" }}>
        <For each={state().layerRows}>
          {(item) => {
            const id = item.id;
            const layer = () => state().document.layers.find((layer) => layer.id === id)!;
            let clickMode: SelectionMode = "replace";
            const rememberModifiers = (event: QuickGuiEvent) => {
              const input = mouseEventFromEvent(event) ?? keyEventFromEvent(event);
              clickMode = input?.shift
                ? "range"
                : input?.control || input?.meta
                  ? "toggle"
                  : "replace";
            };
            return (
              <View
                onPointer={(event) => reorder(id, event)}
                style={{
                  ...row,
                  gap: 4,
                  height: 40,
                  flexShrink: 0,
                  paddingLeft: 5 + item.depth * 14,
                  paddingRight: 6,
                  borderRadius: 5,
                  position: "relative",
                  cursor: "grab",
                  bg: state().isSelected(id) ? "#363e58" : "#282c35",
                  ...(drop()?.id === id && drop()?.side === "into"
                    ? { borderWidth: 1, borderColor: colors.accent }
                    : {}),
                }}
              >
                <Show when={drop()?.id === id && drop()?.side !== "into"}>
                  <View
                    style={{
                      position: "absolute",
                      left: 0,
                      right: 0,
                      height: 2,
                      bg: colors.accent,
                      ...(drop()?.side === "above" ? { top: 0 } : { bottom: 0 }),
                    }}
                  />
                </Show>
                <Show when={layer().content.kind === "group"}>
                  <Button
                    ariaLabel={`${item.collapsed ? "Expand" : "Collapse"} ${layer().name}`}
                    disabled={props.busy}
                    onClick={() => props.act(() => state().toggleGroupExpansion(id))}
                    style={{ width: 16, height: 30, color: colors.muted }}
                  >
                    <Text>{item.collapsed ? "▸" : "▾"}</Text>
                  </Button>
                </Show>
                <View
                  ariaLabel={`Drag ${layer().name}`}
                  tooltip="Drag rows to reorder; drop on a folder to file inside it"
                  style={{
                    ...row,
                    justifyContent: "center",
                    width: 14,
                    height: 30,
                    flexShrink: 0,
                    cursor: "grab",
                  }}
                >
                  <Icon name="grip" size={14} color={colors.muted} />
                </View>
                <Button
                  ariaLabel={`Toggle ${layer().name} visibility`}
                  disabled={props.busy}
                  onClick={() =>
                    props.act(() => {
                      state().select(id);
                      state().updateLayer({ visible: !layer().visible });
                    })
                  }
                  style={{
                    ...row,
                    justifyContent: "center",
                    width: 20,
                    height: 30,
                    flexShrink: 0,
                    color: item.visible ? colors.accent : colors.muted,
                  }}
                >
                  <Icon name={layer().visible ? "eye" : "eyeOff"} size={16} />
                </Button>
                <Button
                  ariaLabel={`Select ${layer().name}`}
                  disabled={props.busy}
                  onMouseDown={(event) => {
                    rememberModifiers(event);
                    // The core's own multi-click count starts text editing on a second press.
                    const mouse = mouseEventFromEvent(event);
                    if (mouse && (mouse.clickCount ?? 0) >= 2 && layer().content.kind === "text" && !props.busy)
                      props.act(() => state().editText({ id }));
                  }}
                  onKeyDown={rememberModifiers}
                  onClick={() => {
                    props.act(() => state().select(id, clickMode));
                    clickMode = "replace";
                  }}
                  style={{
                    ...row,
                    flexGrow: 1,
                    minWidth: 0,
                    gap: 7,
                    height: 30,
                    color: colors.text,
                  }}
                >
                  <Icon
                    name={
                      layer().content.kind === "group"
                        ? "layers"
                        : layer().content.kind === "text"
                        ? "text"
                        : layer().content.kind === "image"
                          ? "image"
                          : layer().content.kind === "paint"
                            ? "brush"
                            : "layers"
                    }
                    size={16}
                    color={colors.accent}
                  />
                  <Text
                    style={{
                      fontSize: 12,
                      lineHeight: "16px",
                      whiteSpace: "nowrap",
                      textOverflow: "ellipsis",
                      flexGrow: 1,
                    }}
                  >
                    {layer().name}
                  </Text>
                  <Show when={layer().locked}>
                    <Icon name="lock" size={14} color={colors.muted} />
                  </Show>
                </Button>
                <Show when={layer().mask}>
                  <Button
                    ariaLabel={`Edit ${layer().name} mask`}
                    tooltip={layer().mask?.enabled ? "Edit layer mask" : "Mask disabled"}
                    disabled={props.busy}
                    onClick={() =>
                      props.act(() => {
                        state().select(id);
                        state().setPaintTarget("mask");
                      })
                    }
                    style={{
                      ...row,
                      justifyContent: "center",
                      width: 24,
                      height: 26,
                      flexShrink: 0,
                      borderRadius: 4,
                      bg:
                        state().selectedId === id && state().paintTarget === "mask"
                          ? "#316357"
                          : "#1b1d23",
                      opacity: layer().mask?.enabled ? 1 : 0.4,
                    }}
                  >
                    <Icon name="mask" size={16} />
                  </Button>
                </Show>
              </View>
            );
          }}
        </For>
        <Show when={!state().document.layers.length}>
          <Text style={{ fontSize: 12, color: colors.muted, padding: 12 }}>
            Import an image or add a layer to begin.
          </Text>
        </Show>
      </View>
      <View style={{ ...row, gap: 5 }}>
        <Action label="Group" disabled={props.busy || !state().selectedIds.length} onClick={() => props.act(() => state().groupSelected())} />
        <Action
          label="Raise layers"
          icon="up"
          iconOnly
          tooltip="Raise selected layers"
          onClick={() => props.act(() => state().reorder(1))}
          disabled={props.busy || !state().selectedLayers.some((layer) => !layer.locked)}
        />
        <Action
          label="Lower layers"
          icon="down"
          iconOnly
          tooltip="Lower selected layers"
          onClick={() => props.act(() => state().reorder(-1))}
          disabled={props.busy || !state().selectedLayers.some((layer) => !layer.locked)}
        />
        <Action
          label="Duplicate"
          onClick={() => props.act(() => state().duplicate())}
          disabled={props.busy || !state().selectedIds.length}
        />
        <Action
          label="Delete layers"
          icon="trash"
          iconOnly
          tooltip="Delete selected unlocked layers"
          onClick={() => props.act(() => state().remove())}
          disabled={props.busy || !state().selectedLayers.some((layer) => !layer.locked)}
        />
      </View>
      <Show when={state().selectedIds.length > 0}>
        <View style={{ ...row, gap: 4, flexWrap: "wrap" }}>
          <Action label="Out of folder" disabled={props.busy || !state().selectedLayers.some((layer) => layer.parentId)} onClick={() => props.act(() => state().moveToGroup())} />
          <For each={state().document.layers.filter((layer) => layer.content.kind === "group" && !state().isSelected(layer.id))}>
            {(folder) => <Action label={`Into ${folder.name}`} disabled={props.busy} onClick={() => props.act(() => state().moveToGroup(folder.id))} />}
          </For>
        </View>
      </Show>
      <Show when={state().selectedIds.length > 1}>
        <Text style={{ fontSize: 11, color: colors.accent, lineHeight: "16px" }}>
          {state().selectedIds.length} layers selected
        </Text>
        <Text style={{ fontSize: 11, color: colors.muted, lineHeight: "16px" }}>
          Drag with Move or use arrow keys to move together. Select one layer to resize, rotate, or
          paint. Locked layers stay in place.
        </Text>
      </Show>
    </Section>
  );
}
