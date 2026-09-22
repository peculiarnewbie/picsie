import { Svg } from "@quickgui/solid";

// Every icon shares one view box, stroke weight, and optical size.
const paths = {
  check: '<path d="m5 12 4 4 10-10"/>',
  move: '<path d="m5 3 14 9-7 1-3 7-4-17Z"/>',
  brush: '<path d="m9 14 8-10a2 2 0 0 1 3 3l-10 8M9 14c-4-2-6 1-5 4-1 1-2 2-2 2 7 1 10-2 7-6Z"/>',
  eraser:
    '<path d="m14 4 6 6a2 2 0 0 1 0 3l-7 7H7l-4-4a2 2 0 0 1 0-3l8-9a2 2 0 0 1 3 0ZM7 9l9 8M13 20h8"/>',
  rectangle: '<rect x="4" y="4" width="16" height="16" rx="1"/>',
  ellipse: '<circle cx="12" cy="12" r="8"/>',
  text: '<path d="M4 6V4h16v2M12 4v16M8 20h8"/>',
  hand: '<path d="M8 12V5a2 2 0 0 1 4 0v7M12 7a2 2 0 0 1 4 0v5M16 9a2 2 0 0 1 4 0v6c0 4-3 6-6 6h-2c-2 0-3-1-4-2l-4-5c-2-3 0-5 2-3l2 2"/>',
  eyedropper: '<path d="m14 5 5 5M13 6l-9 9v4h4l9-9M15 4l2-2 5 5-2 2M3 20l2-2"/>',
  image:
    '<rect x="3" y="3" width="18" height="18" rx="2"/><circle cx="8" cy="8" r="1.5"/><path d="m3 16 5-5 4 4 3-3 6 6"/>',
  layers: '<path d="m12 3 9 5-9 5-9-5 9-5ZM3 12l9 5 9-5M3 16l9 5 9-5"/>',
  eye: '<path d="M2 12s4-7 10-7 10 7 10 7-4 7-10 7S2 12 2 12Z"/><circle cx="12" cy="12" r="3"/>',
  eyeOff:
    '<path d="m3 3 18 18M9 5c6-2 11 4 13 7a20 20 0 0 1-4 4M6 6a19 19 0 0 0-4 6s4 7 10 7c2 0 3 0 5-1"/>',
  lock: '<rect x="5" y="10" width="14" height="11" rx="2"/><path d="M8 10V7a4 4 0 0 1 8 0v3M12 14v3"/>',
  undo: '<path d="M4 5v6h6M4 11a8 8 0 1 1 1 7"/>',
  redo: '<path d="M20 5v6h-6M20 11a8 8 0 1 0-1 7"/>',
  up: '<path d="m6 10 6-6 6 6M12 4v16"/>',
  down: '<path d="m6 14 6 6 6-6M12 4v16"/>',
  plus: '<path d="M5 12h14M12 5v14"/>',
  minus: '<path d="M5 12h14"/>',
  trash: '<path d="M3 6h18M9 6V3h6v3M5 6l1 15h12l1-15M10 10v7M14 10v7"/>',
  duplicate: '<rect x="8" y="8" width="13" height="13" rx="2"/><path d="M16 8V3H3v13h5"/>',
  chevron: '<path d="m6 9 6 6 6-6"/>',
  export: '<path d="M14 3h7v7M21 3l-11 11M10 4H4v16h16v-6"/>',
  grip: '<path d="M8 5h1M15 5h1M8 12h1M15 12h1M8 19h1M15 19h1"/>',
  mask: '<rect x="3" y="3" width="18" height="18" rx="2"/><path d="M12 3v18"/><path d="M12 3h9v18h-9Z" fill="white"/>',
} as const;

export type IconName = keyof typeof paths;
export function Icon(props: { name: IconName; size?: number; color?: string }) {
  return (
    <Svg
      source={`<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="white" stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round">${paths[props.name]}</svg>`}
      style={{
        width: props.size ?? 18,
        height: props.size ?? 18,
        flexShrink: 0,
        color: props.color,
      }}
    />
  );
}
