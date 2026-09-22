import { app, Window } from "@quickgui/native";
import { createRenderer } from "@quickgui/solid";
import { Editor } from "./src/engine/editor.ts";
import { Shell } from "./src/ui/shell.tsx";
import { canQuit } from "./src/ui/lifecycle.ts";

function openWindow(editor: Editor = new Editor(), path?: string) {
  new Window({
    title: "Electropic — Compositor",
    width: 1280,
    height: 860,
    minimumWidth: 960,
    minimumHeight: 640,
    background: "#1b1d23",
    appearance: "dark",
    renderer: createRenderer(() => <Shell editor={editor} path={path} openWindow={openWindow} />),
  });
}

app.on("reopen", ({ hasVisibleWindows }) => {
  if (!hasVisibleWindows) openWindow();
});
let confirmingQuit = false;
app.on("beforeQuit", () => {
  if (confirmingQuit) return;
  confirmingQuit = true;
  void (async () => {
    try {
      if (await canQuit()) await app.quit({ force: true });
    } finally {
      confirmingQuit = false;
    }
  })();
});
await app.whenReady();
openWindow();
