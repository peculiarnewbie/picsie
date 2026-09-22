import { Text, View } from "@quickgui/solid";
import { Show } from "solid-js";
import type { Editor } from "../engine/editor.ts";
import { Action, Section, colors, row } from "./controls.tsx";

export function MaskPanel(props: {
  editor: () => Editor;
  busy: boolean;
  act: (callback: () => void) => void;
}) {
  const state = props.editor;
  const disabled = () => props.busy || state().selected?.locked;
  return (
    <Section title="Layer mask">
      <Show
        when={state().selected?.mask}
        fallback={
          <Action
            label="Add mask"
            icon="mask"
            disabled={disabled()}
            onClick={() => props.act(() => state().addMask())}
          />
        }
      >
        {(mask) => (
          <>
            <View style={row}>
              <Action
                label="Layer pixels"
                active={state().paintTarget === "content"}
                disabled={props.busy}
                onClick={() => props.act(() => state().setPaintTarget("content"))}
              />
              <Action
                label="Mask"
                icon="mask"
                active={state().paintTarget === "mask"}
                disabled={disabled()}
                onClick={() => props.act(() => state().setPaintTarget("mask"))}
              />
            </View>
            <Text style={{ fontSize: 11, lineHeight: "16px", minHeight: 32, color: colors.muted }}>
              {mask().enabled
                ? "Paint Hide to conceal; Reveal to restore. X swaps modes. Eraser reverses the mode."
                : "Mask disabled. Enable it to paint."}
            </Text>
            <View style={row}>
              <Action
                label="Reveal all"
                disabled={disabled()}
                onClick={() => props.act(() => state().resetMask("reveal"))}
              />
              <Action
                label="Hide all"
                disabled={disabled()}
                onClick={() => props.act(() => state().resetMask("hide"))}
              />
            </View>
            <View style={row}>
              <Action
                label={mask().enabled ? "Disable" : "Enable"}
                disabled={disabled()}
                onClick={() =>
                  props.act(() =>
                    state().updateLayer({ mask: { ...mask(), enabled: !mask().enabled } }),
                  )
                }
              />
              <Action
                label="Remove mask"
                disabled={disabled()}
                onClick={() => props.act(() => state().removeMask())}
              />
            </View>
          </>
        )}
      </Show>
      <Show when={!state().selected?.mask}>
        <Text style={{ fontSize: 11, lineHeight: "16px", color: colors.muted }}>
          Hide or restore areas while keeping the original pixels.
        </Text>
      </Show>
    </Section>
  );
}
