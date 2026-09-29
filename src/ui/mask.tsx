import { Select, Text, View } from "@quickgui/solid";
import { Show } from "solid-js";
import { Icon } from "./icons.tsx";
import type { Editor } from "../engine/editor.ts";
import { Action, Section, colors, row, inputStyle, pickerAppearance } from "./controls.tsx";

export function MaskPanel(props: {
  editor: () => Editor;
  busy: boolean;
  act: (callback: () => void) => void;
}) {
  const state = props.editor;
  const disabled = () => props.busy || state().selected?.locked;
  return (
    <>
      <Section title={state().selected?.content.kind === "group" ? "Folder mask" : "Layer mask"}>
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
                  label={state().selected?.content.kind === "group" ? "Folder" : "Layer pixels"}
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
              <Text
                style={{ fontSize: 11, lineHeight: "16px", minHeight: 32, color: colors.muted }}
              >
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
              <Show when={state().selected?.content.kind !== "group"}>
                <View style={row}>
                  <Action
                    label={mask().linked ? "Linked to layer" : "Independent mask"}
                    active={mask().linked}
                    disabled={disabled()}
                    onClick={() => props.act(() => state().toggleMaskLink())}
                  />
                </View>
              </Show>
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
      <Show when={state().selected?.content.kind !== "group"}>
        <Section title="Clipping">
          <Action
            label={state().selected?.maskSourceId ? "Release clipping mask" : "Clip to layer below"}
            disabled={props.busy || !state().canToggleClipping}
            onClick={() => props.act(() => state().toggleClippingMask())}
          />
          <Select
            ariaLabel="Live mask source"
            appearance={pickerAppearance}
            value={state().selected?.maskSourceId ?? ""}
            items={state().maskSourceIds.map((id) => ({
              value: id,
              label: state().document.layers.find((l) => l.id === id)?.name ?? id,
            }))}
            disabled={disabled() || !state().maskSourceIds.length}
            onValueChange={(id) => {
              if (id) props.act(() => state().linkMask(id));
            }}
            style={{
              ...inputStyle,
              ...row,
              width: "100%",
              flexGrow: 0,
              justifyContent: "space-between",
            }}
          >
            <Select.Value>
              <Text>
                {state().document.layers.find((l) => l.id === state().selected?.maskSourceId)
                  ?.name ?? "Choose alpha source…"}
              </Text>
            </Select.Value>
            <Select.Icon>
              <Icon name="chevron" size={14} />
            </Select.Icon>
            <Select.Positioner side="top" align="start" sideOffset={4} />
          </Select>
        </Section>
      </Show>
    </>
  );
}
