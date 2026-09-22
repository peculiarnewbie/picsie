import { mkdir, copyFile } from "node:fs/promises";
import { Editor } from "../src/engine/editor.ts";
const editor = new Editor();
try {
  await mkdir("artifacts", { recursive: true });
  await editor.export("artifacts/demo.png", "png");
  await copyFile(await editor.preview(), "artifacts/canvas-preview.tiff");
  console.log("Rust rendered artifacts/demo.png and artifacts/canvas-preview.tiff.");
} finally {
  editor.close();
}
