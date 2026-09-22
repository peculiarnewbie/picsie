import type { Window } from "@quickgui/native";
import type { Document } from "../engine/types.ts";

const closeGuards = new Map<
  Window,
  { confirm: () => Promise<boolean>; document: () => Document }
>();

export function registerCloseGuard(
  window: Window,
  confirm: () => Promise<boolean>,
  document: () => Document,
) {
  closeGuards.set(window, { confirm, document });
  return () => {
    closeGuards.delete(window);
  };
}

/** Confirm every open document before allowing the application-level Quit action. */
export async function canQuit(): Promise<boolean> {
  const confirmed = new Map<Window, Document>();
  for (const [window, guard] of closeGuards) {
    if (window.closed) continue;
    if (!(await guard.confirm())) return false;
    confirmed.set(window, guard.document());
  }
  // A user can return to an earlier window while a later window's dialog is open.
  for (const [window, document] of confirmed) {
    if (!window.closed && closeGuards.get(window)?.document() !== document) return false;
  }
  return true;
}
