// QuickGUI adaptation of Compositor UI/CanvasSizeSheet.swift (MIT).
import { Button, Checkbox, Dialog, Select, Text, View } from "@quickgui/solid";
import { For, Show, createSignal } from "solid-js";
import { CanvasSizeDraft, anchorNames } from "./canvas-size-draft.ts";
import type { CanvasSizeOptions } from "../engine/types.ts";
import type { Document } from "../engine/types.ts";
import { Action, Field, colors, column, inputStyle, pickerAppearance, row } from "./controls.tsx";
import { Icon } from "./icons.tsx";

function Picker(props: {
  label: string;
  value: string;
  items: Record<string, string>;
  onChange: (value: string) => void;
}) {
  return (
    <View style={{ ...column, gap: 5, flexGrow: 1, width: 0 }}>
      <Text style={{ color: colors.muted, fontSize: 10, lineHeight: "14px" }}>{props.label}</Text>
      <Select
        ariaLabel={props.label}
        value={props.value}
        items={props.items}
        onValueChange={(value) => {
          if (value) props.onChange(value);
        }}
        appearance={pickerAppearance}
        style={{ ...inputStyle, ...row, flexGrow: 0, justifyContent: "space-between" }}
      >
        <Select.Value>
          <Text>{props.items[props.value]}</Text>
        </Select.Value>
        <Select.Icon>
          <Icon name="chevron" size={14} />
        </Select.Icon>
        <Select.Positioner side="top" align="start" sideOffset={4} />
      </Select>
    </View>
  );
}

function Toggle(props: { label: string; checked: boolean; onChange: (checked: boolean) => void }) {
  return (
    <Checkbox
      checked={props.checked}
      onCheckedChange={props.onChange}
      ariaLabel={props.label}
      style={{ ...row, height: 24, color: colors.text }}
    >
      <Checkbox.Indicator
        style={{
          ...row,
          justifyContent: "center",
          width: 16,
          height: 16,
          borderRadius: 3,
          borderWidth: 1,
          borderColor: props.checked ? colors.accent : colors.muted,
          bg: props.checked ? colors.accent : "#191c22",
        }}
      >
        <Show when={props.checked}>
          <Icon name="check" size={12} color="#171b2c" />
        </Show>
      </Checkbox.Indicator>
      <Text style={{ fontSize: 12, lineHeight: "16px" }}>{props.label}</Text>
    </Checkbox>
  );
}

export function CanvasSizeDialog(props: {
  document: Document;
  foreground: string;
  onClose: () => void;
  onApply: (options: CanvasSizeOptions) => Promise<void>;
}) {
  const draft = new CanvasSizeDraft(props.document);
  const [revision, setRevision] = createSignal(0);
  const state = () => {
    revision();
    return draft;
  };
  const update = (edit: () => void) => {
    edit();
    setRevision((value) => value + 1);
    setError("");
  };
  const [anchor, setAnchor] = createSignal(4);
  const [extension, setExtension] = createSignal("transparent");
  const [custom, setCustom] = createSignal("#ffffff");
  const [applying, setApplying] = createSignal(false);
  const [error, setError] = createSignal("");
  const displayed = (width: boolean) => Math.round(state().displayed(width) * 1000) / 1000;
  const dimension = (value: string, width: boolean) =>
    update(() => draft.set(value.trim() ? Number(value) : NaN, width));
  const apply = async () => {
    if (!draft.valid || applying()) return;
    const fill =
      extension() === "transparent"
        ? undefined
        : extension() === "foreground"
          ? props.foreground
          : extension() === "black"
            ? "#000000"
            : extension() === "white"
              ? "#ffffff"
              : custom();
    setApplying(true);
    try {
      await props.onApply({
        width: Math.round(draft.width),
        height: Math.round(draft.height),
        anchor: anchor(),
        fill,
      });
      props.onClose();
    } catch (error) {
      setError(error instanceof Error ? error.message : String(error));
    } finally {
      setApplying(false);
    }
  };
  return (
    <Dialog.Root
      open
      onOpenChange={(open) => {
        if (!open && !applying()) props.onClose();
      }}
    >
      <Dialog.Portal style={{ display: "flex", alignItems: "center", justifyContent: "center" }}>
        <Dialog.Backdrop
          style={{ position: "absolute", top: 0, left: 0, right: 0, bottom: 0, bg: "#00000099" }}
        />
        <Dialog.Popup
          style={{
            ...column,
            width: 440,
            padding: 20,
            borderRadius: 12,
            bg: colors.panel,
            color: colors.text,
            borderWidth: 1,
            borderColor: colors.line,
          }}
        >
          <Dialog.Title style={{ fontSize: 20, lineHeight: "26px", fontWeight: 600 }}>
            Canvas Size
          </Dialog.Title>
          <Dialog.Description style={{ fontSize: 12, lineHeight: "16px", color: colors.muted }}>
            Current: {draft.originalWidth} × {draft.originalHeight} pixels
          </Dialog.Description>
          <View style={row}>
            <Picker
              label="Units"
              value={state().unit}
              items={{ pixels: "Pixels", percent: "Percent" }}
              onChange={(value) =>
                update(() => {
                  if (value === "pixels" || value === "percent") draft.unit = value;
                })
              }
            />
            <Field
              label="Width"
              value={displayed(true)}
              onCommit={(value) => dimension(value, true)}
            />
            <Field
              label="Height"
              value={displayed(false)}
              onCommit={(value) => dimension(value, false)}
            />
          </View>
          <View style={{ ...column, gap: 4 }}>
            <Toggle
              label="Relative to current dimensions"
              checked={state().relative}
              onChange={(value) =>
                update(() => {
                  draft.relative = value;
                })
              }
            />
            <Toggle
              label="Lock original aspect ratio"
              checked={state().locked}
              onChange={(value) =>
                update(() => {
                  draft.locked = value;
                  if (value) draft.set(draft.displayed(true), true);
                })
              }
            />
          </View>
          <Text
            style={{
              fontSize: 12,
              lineHeight: "16px",
              color: state().valid ? colors.accent : "#ffb49b",
              minHeight: 32,
            }}
          >
            {state().valid
              ? `New: ${Math.round(state().width)} × ${Math.round(state().height)} pixels`
              : "Use 1–8192 pixels per side, up to 24 megapixels."}
          </Text>
          <View style={{ ...row, alignItems: "center", gap: 20 }}>
            <View style={{ ...column, gap: 4, flexShrink: 0 }}>
              <Text style={{ color: colors.muted, fontSize: 10, lineHeight: "14px" }}>Anchor</Text>
              <For each={[0, 1, 2]}>
                {(rowIndex) => (
                  <View style={{ ...row, gap: 4 }}>
                    <For each={[0, 1, 2]}>
                      {(columnIndex) => {
                        const index = rowIndex * 3 + columnIndex;
                        return (
                          <Button
                            ariaLabel={anchorNames[index]}
                            tooltip={anchorNames[index]}
                            onClick={() => setAnchor(index)}
                            style={{
                              ...row,
                              justifyContent: "center",
                              width: 30,
                              height: 30,
                              borderRadius: 4,
                              borderWidth: 1,
                              borderColor: anchor() === index ? colors.accent : colors.line,
                              bg: anchor() === index ? "#3a4160" : "#191c22",
                            }}
                          >
                            <View
                              style={{
                                width: 8,
                                height: 8,
                                borderRadius: 4,
                                borderWidth: 1,
                                borderColor: anchor() === index ? colors.accent : colors.muted,
                                bg: anchor() === index ? colors.accent : "#191c22",
                              }}
                            />
                          </Button>
                        );
                      }}
                    </For>
                  </View>
                )}
              </For>
            </View>
            <View style={{ ...column, flexGrow: 1, width: 0, gap: 8 }}>
              <Text style={{ fontSize: 12, lineHeight: "16px", fontWeight: 600 }}>
                {anchorNames[anchor()]}
              </Text>
              <Text style={{ fontSize: 12, lineHeight: "18px", color: colors.muted }}>
                Keeps this point fixed. Artwork is not scaled; cropped content remains outside the
                canvas.
              </Text>
            </View>
          </View>
          <View style={row}>
            <Picker
              label="Canvas extension"
              value={extension()}
              items={{
                transparent: "Transparent",
                foreground: "Foreground",
                black: "Black",
                white: "White",
                custom: "Custom",
              }}
              onChange={setExtension}
            />
            <Show when={extension() === "custom"}>
              <Field label="Extension color" value={custom()} onCommit={setCustom} />
            </Show>
          </View>
          <Show when={error()}>
            <Text style={{ fontSize: 11, lineHeight: "16px", color: "#ffb49b" }}>{error()}</Text>
          </Show>
          <View style={{ ...row, justifyContent: "flex-end", marginTop: 6 }}>
            <Action label="Cancel" disabled={applying()} onClick={props.onClose} />
            <Action
              label="Resize canvas"
              primary
              disabled={!state().valid || applying()}
              onClick={apply}
            />
          </View>
        </Dialog.Popup>
      </Dialog.Portal>
    </Dialog.Root>
  );
}
