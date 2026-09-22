import {
  Button,
  Input,
  Text,
  TextArea,
  View,
  type NativeStyle,
  type PickerAppearance,
} from "@quickgui/solid";
import { createEffect, untrack } from "solid-js";
import { Show } from "solid-js";
import { Icon, type IconName } from "./icons.tsx";

export const colors = {
  bg: "#1b1d23",
  panel: "#22252d",
  line: "#343843",
  text: "#e9eaf0",
  muted: "#979faf",
  accent: "#a5b4fc",
};
export const row: NativeStyle = {
  display: "flex",
  flexDirection: "row",
  alignItems: "center",
  gap: 8,
};
export const pickerAppearance: PickerAppearance = {
  width: 200,
  rowHeight: 30,
  maxVisibleRows: 6,
  fontSize: 12,
  radius: 5,
  background: colors.panel,
  color: colors.text,
  highlightBackground: "#3a4160",
  highlightColor: colors.text,
  selectedBackground: "#363e58",
  mutedColor: colors.muted,
};
export const column: NativeStyle = { display: "flex", flexDirection: "column", gap: 10 };
export const fieldRow: NativeStyle = { ...row, alignItems: "flex-end" };
export const inputStyle: NativeStyle = {
  height: 30,
  paddingLeft: 8,
  paddingRight: 8,
  bg: "#191c22",
  color: colors.text,
  fontSize: 12,
  borderRadius: 5,
  borderWidth: 1,
  borderColor: colors.line,
  flexGrow: 1,
  minWidth: 0,
};

export function Action(props: {
  label: string;
  onClick: () => void;
  disabled?: boolean;
  active?: boolean;
  tooltip?: string;
  primary?: boolean;
  style?: NativeStyle;
  icon?: IconName;
  iconOnly?: boolean;
}) {
  return (
    <Button
      ariaLabel={props.label}
      tooltip={props.tooltip}
      disabled={props.disabled}
      onClick={props.onClick}
      style={{
        ...row,
        justifyContent: "center",
        gap: 6,
        height: 30,
        paddingLeft: props.iconOnly ? 0 : 11,
        paddingRight: props.iconOnly ? 0 : 11,
        ...(props.iconOnly ? { width: 30 } : {}),
        borderRadius: 5,
        fontSize: 12,
        whiteSpace: "nowrap",
        flexShrink: 0,
        bg: props.primary ? colors.accent : props.active ? "#3a4160" : "#2c303a",
        color: props.primary ? "#171b2c" : props.active ? "#cbd5ff" : colors.text,
        opacity: props.disabled ? 0.4 : 1,
        hover: { bg: props.primary ? "#bfcbff" : "#3c4250" },
        ...props.style,
      }}
    >
      <Show when={props.icon}>{(name) => <Icon name={name()} size={18} />}</Show>
      <Show when={!props.iconOnly}>
        <Text style={{ fontSize: 12, lineHeight: "16px", whiteSpace: "nowrap" }}>
          {props.label}
        </Text>
      </Show>
    </Button>
  );
}

/** Commit on Enter or blur; incomplete numeric strings never enter the document. */
export function Field(props: {
  label: string;
  value: string | number;
  onCommit: (value: string) => void;
  disabled?: boolean;
  wide?: boolean;
  inline?: boolean;
}) {
  let draft = untrack(() => String(props.value));
  createEffect(
    () => props.value,
    (value) => {
      draft = String(value);
    },
  );
  const commit = () => {
    if (draft === String(props.value)) return;
    props.onCommit(draft);
  };
  return (
    <View
      style={{
        ...column,
        flexDirection: props.inline ? "row" : "column",
        alignItems: props.inline ? "center" : "stretch",
        gap: 5,
        flexGrow: props.wide ? 0 : 1,
        flexBasis: "auto",
        width: 0,
        minHeight: props.inline ? 30 : 49,
        minWidth: 0,
        ...(props.wide ? { width: "100%" } : {}),
      }}
    >
      <Text style={{ color: colors.muted, fontSize: 10, lineHeight: "14px", flexShrink: 0 }}>
        {props.label}
      </Text>
      <Input
        ariaLabel={props.label}
        value={String(props.value)}
        disabled={props.disabled}
        onInput={(e) => {
          draft = e.value ?? "";
        }}
        onSubmit={commit}
        onBlur={commit}
        style={{ ...inputStyle, width: props.inline ? 60 : "100%", flexGrow: props.inline ? 1 : 0 }}
      />
    </View>
  );
}

/** Let the native text engine own keystrokes; echoing asynchronous edits can overwrite newer text. */
export function TextEditor(props: {
  value: string;
  disabled?: boolean;
  onCommit: (value: string) => void;
}) {
  let draft = untrack(() => props.value);
  createEffect(
    () => props.value,
    (value) => {
      draft = value;
    },
  );
  return (
    <TextArea
      ariaLabel="Layer text"
      value={props.value}
      disabled={props.disabled}
      onInput={(event) => {
        draft = event.value ?? "";
      }}
      onBlur={() => {
        if (draft !== props.value) props.onCommit(draft);
      }}
      style={{ ...inputStyle, height: 80 }}
    />
  );
}

export function Section(props: { title: string; children: unknown }) {
  return (
    <View style={{ ...column, padding: 14, borderBottomWidth: 1, borderColor: colors.line }}>
      <Text style={{ fontSize: 10, fontWeight: 600, color: colors.muted, letterSpacing: 1.4 }}>
        {props.title.toUpperCase()}
      </Text>
      {props.children}
    </View>
  );
}
