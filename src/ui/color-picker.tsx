import { Dialog, Input, Text, View, capturedPointerFromEvent } from "@quickgui/solid";
import type { NativeNode, QuickGuiEvent } from "@quickgui/native";
import { For, createSignal } from "solid-js";
import { fromHSB, formatHex, parseHex, quantized, toHSB, type HSB, type RGB } from "./color-math.ts";
import { Action, Field, colors, column, inputStyle, row } from "./controls.tsx";

/**
 * Compositor's ColorPickerSheet: a saturation/brightness square, a hue strip, live preview,
 * integer RGB fields, and a hex field. Nothing is written until OK (Cancel discards).
 */
export function ColorPickerDialog(props: {
  /** What the picker edits, as in upstream's `ColorPickerTarget` title. */
  target: string;
  color: string;
  onCommit: (hex: string) => void;
  onClose: () => void;
}) {
  const start = parseHex(props.color) ?? { r: 0, g: 0, b: 0 };
  const [hsb, setHsb] = createSignal<HSB>(toHSB(start));
  const [hex, setHex] = createSignal(formatHex(start));
  const current = () => fromHSB(hsb());
  const sync = (color: RGB) => {
    const value = quantized(color);
    // Grays keep the previous hue and black keeps saturation, as Photoshop's fields do.
    setHsb(toHSB(value, hsb()));
    setHex(formatHex(value));
  };
  const drag = (event: QuickGuiEvent, area: "sv" | "hue") => {
    const pointer = capturedPointerFromEvent(event);
    if (pointer.phase === "cancel" || (pointer.phase !== "down" && pointer.phase !== "move")) return;
    const { x, y } = pointer.localPosition;
    const state = hsb();
    if (area === "sv") {
      sync(
        fromHSB({
          h: state.h,
          s: Math.min(1, Math.max(0, x / 256)),
          v: Math.min(1, Math.max(0, 1 - y / 256)),
        }),
      );
    } else {
      sync(fromHSB({ ...state, h: (1 - y / 256) * 360 }));
    }
  };
  const commitHex = (text: string) => {
    const value = parseHex(text);
    if (value) sync(value);
    else setHex(formatHex(current()));
  };
  const fields: Array<{ key: keyof RGB; label: string }> = [
    { key: "r", label: "R" },
    { key: "g", label: "G" },
    { key: "b", label: "B" },
  ];
  return (
    <Dialog.Root open onOpenChange={(open: boolean) => !open && props.onClose()}>
      <Dialog.Portal style={{ display: "flex", alignItems: "center", justifyContent: "center" }}>
        <Dialog.Backdrop
          style={{ position: "absolute", top: 0, right: 0, bottom: 0, left: 0, bg: "#00000099" }}
        />
        <Dialog.Popup
          style={{
            ...column,
            width: 480,
            padding: 20,
            borderRadius: 12,
            bg: colors.panel,
            color: colors.text,
            borderWidth: 1,
            borderColor: colors.line,
          }}
        >
          <Dialog.Title style={{ fontSize: 16, fontWeight: 600 }}>
            Color Picker ({props.target})
          </Dialog.Title>
          <View style={{ ...row, gap: 14, alignItems: "flex-start" }}>
            {/* White → hue under transparent → black, the saturation/brightness square. */}
            <View
              onPointer={(event) => drag(event, "sv")}
              style={{ width: 256, height: 256, flexShrink: 0, position: "relative", cursor: "crosshair" }}
            >
              <View
                style={{
                  position: "absolute",
                  top: 0,
                  left: 0,
                  width: 256,
                  height: 256,
                  bgGradient: `linear-gradient(to right, rgb(255, 255, 255), hsl(${Math.round(hsb().h)}, 100%, 50%))`,
                }}
              />
              <View
                style={{
                  position: "absolute",
                  top: 0,
                  left: 0,
                  width: 256,
                  height: 256,
                  bgGradient: "linear-gradient(to bottom, rgba(0, 0, 0, 0), rgba(0, 0, 0, 1))",
                }}
              />
              <View
                style={{
                  position: "absolute",
                  width: 12,
                  height: 12,
                  borderRadius: 6,
                  borderWidth: 2,
                  borderColor: "#ffffff",
                  boxShadow: "0 0 0 1px #000000aa",
                  left: Math.max(0, Math.min(250, hsb().s * 256 - 6)),
                  top: Math.max(0, Math.min(250, (1 - hsb().v) * 256 - 6)),
                }}
              />
            </View>
            <View
              onPointer={(event) => drag(event, "hue")}
              style={{ width: 20, height: 256, flexShrink: 0, position: "relative", cursor: "crosshair" }}
            >
              <View
                style={{
                  position: "absolute",
                  top: 0,
                  left: 0,
                  width: 20,
                  height: 256,
                  bgGradient:
                    "linear-gradient(to bottom, #ff0000, #ffff00, #00ff00, #00ffff, #0000ff, #ff00ff, #ff0000)",
                }}
              />
              <View
                style={{
                  position: "absolute",
                  left: -3,
                  width: 26,
                  height: 3,
                  bg: "#ffffff",
                  boxShadow: "0 0 0 1px #000000aa",
                  top: Math.max(0, Math.min(253, (1 - hsb().h / 360) * 256 - 1.5)),
                }}
              />
            </View>
            <View style={{ ...column, gap: 10, width: 110 }}>
              <View style={{ ...column, gap: 4 }}>
                <Text style={{ color: colors.muted, fontSize: 10 }}>New color</Text>
                <View
                  style={{
                    width: 64,
                    height: 64,
                    borderRadius: 6,
                    bg: `#${hex()}`,
                    borderWidth: 1,
                    borderColor: colors.line,
                  }}
                />
              </View>
              <For each={fields}>
                {(field) => (
                  <Field
                    label={field.label}
                    value={String(current()[field.key])}
                    onCommit={(text) => {
                      const value = Number(text.trim());
                      if (!Number.isFinite(value)) return setHex(formatHex(current()));
                      sync({ ...current(), [field.key]: Math.min(255, Math.max(0, Math.round(value))) });
                    }}
                  />
                )}
              </For>
              <Field label="#" value={hex()} onCommit={commitHex} />
              <View style={{ ...row, gap: 6, justifyContent: "flex-end" }}>
                <Action label="Cancel" onClick={props.onClose} />
                <Action label="OK" primary onClick={() => props.onCommit(hex())} />
              </View>
            </View>
          </View>
          <Text style={{ color: colors.muted, fontSize: 10 }}>
            Hex accepts RRGGBB or RGB, with or without #. Use the Sample tool to pick from the
            canvas.
          </Text>
        </Dialog.Popup>
      </Dialog.Portal>
    </Dialog.Root>
  );
}
