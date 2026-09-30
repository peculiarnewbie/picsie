import { mkdir, readFile, writeFile } from "node:fs/promises";

const { version } = JSON.parse(await readFile("package.json", "utf8"));
const changelog = await readFile("CHANGELOG.md", "utf8");
const heading = new RegExp(`^## ${version.replaceAll(".", "\\.")}(?: - [^\\n]+)?\\r?$`, "m");
const match = heading.exec(changelog);
if (!match) throw new Error(`Missing CHANGELOG.md entry for ${version}`);
const notes = changelog
  .slice(match.index + match[0].length)
  .split(/^## /m)[0]
  .trim();
if (!notes || Buffer.byteLength(notes) > 16 * 1024)
  throw new Error("Release notes must contain 1–16384 bytes.");
await mkdir("artifacts", { recursive: true });
await writeFile("artifacts/release-notes.md", notes + "\n");
