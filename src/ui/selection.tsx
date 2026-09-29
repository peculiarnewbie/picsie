import { Text, View } from "@quickgui/solid";
import { createSignal } from "solid-js";
import type { Editor } from "../engine/editor.ts";
import { Action, Field, Section, colors, row } from "./controls.tsx";

export function SelectionPanel(props: {
  editor: () => Editor;
  busy: boolean;
  act: (callback: () => void) => void;
}) {
  const [amount, setAmount] = createSignal(5);
  const [feather, setFeather] = createSignal(2);
  const state = props.editor;
  const disabled = () => props.busy || !state().pixelSelectionBounds;
  function commit(value: string, maximum: number, set: (value: number) => void) {
    const parsed = Number(value.trim());
    if (Number.isFinite(parsed)) set(Math.max(1, Math.min(maximum, Math.round(parsed))));
  }
  return (
    <Section title="Pixel selection">
      <Text style={{ color: colors.muted, fontSize: 11 }}>
        {state().hasPixelSelection
          ? state().pixelSelectionBounds
            ? "Edits affect the selected area."
            : "The selection is empty."
          : "No selection · edits affect the canvas."}
      </Text>
      <View style={row}>
        <Action
          label="All"
          disabled={props.busy}
          onClick={() => props.act(() => state().selectAllPixels())}
        />
        <Action
          label="Invert"
          disabled={props.busy || !state().hasPixelSelection}
          onClick={() => props.act(() => state().invertSelection())}
        />
        <Action
          label="Deselect"
          disabled={props.busy || !state().hasPixelSelection}
          onClick={() => props.act(() => state().deselectPixels())}
        />
      </View>
      <Field
        wide
        label="Expand / contract (px)"
        value={amount()}
        onCommit={(v) => commit(v, 500, setAmount)}
      />
      <View style={row}>
        <Action
          label="Expand"
          disabled={disabled()}
          onClick={() => props.act(() => state().expandSelection(amount()))}
        />
        <Action
          label="Contract"
          disabled={disabled()}
          onClick={() => props.act(() => state().contractSelection(amount()))}
        />
      </View>
      <View style={row}>
        <Field
          inline
          label="Feather px"
          value={feather()}
          onCommit={(v) => commit(v, 250, setFeather)}
        />
        <Action
          label="Feather"
          disabled={disabled()}
          onClick={() => props.act(() => state().featherSelection(feather()))}
        />
      </View>
      <View style={row}>
        <Action
          label="Fill"
          disabled={props.busy || !state().canEditPixels}
          onClick={() => props.act(() => state().fillSelection())}
        />
        <Action
          label="Clear pixels"
          disabled={props.busy || !state().hasPixelSelection || !state().canEditPixels}
          onClick={() => props.act(() => state().clearSelectedPixels())}
        />
      </View>
    </Section>
  );
}
