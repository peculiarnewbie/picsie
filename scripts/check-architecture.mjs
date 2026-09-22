import { readFile, readdir } from "node:fs/promises";
import { dirname, resolve, posix } from "node:path";
import { fileURLToPath } from "node:url";

const sourceExtension = /\.(?:[cm]?[jt]s|[jt]sx)$/;
const ignored = new Set([
  ".git",
  "node_modules",
  ".quickgui",
  "dist",
  "artifacts",
  "docs",
  "resources",
  "tests",
  "scripts",
  ".github",
  "target",
]);
const pixelApi =
  /\b(?:getImageData|putImageData|createCanvas|createImageBitmap|OffscreenCanvas|ImageData|toDataURL|atob|btoa)\b|\bBuffer\s*[.[]/g;
const pixelStorage =
  /\b(?:Uint(?:8|16|32)(?:Clamped)?Array|Float(?:32|64)Array|ArrayBuffer|DataView)\b/g;

async function applicationFiles(root, folder = "") {
  const files = [];
  for (const entry of await readdir(resolve(root, folder), { withFileTypes: true })) {
    if (folder === "" && ignored.has(entry.name)) continue;
    if (folder.startsWith("crates/") && entry.name === "target") continue;
    const file = posix.join(folder, entry.name);
    if (entry.isSymbolicLink()) throw new Error(`Application symlinks are not allowed: ${file}`);
    if (entry.isDirectory()) files.push(...(await applicationFiles(root, file)));
    else if (sourceExtension.test(file) && file !== "quickgui.config.ts") files.push(file);
  }
  return files;
}

/** Conservative source guard; semantic engine ownership also requires review. */
export async function checkArchitecture(root) {
  const policy = JSON.parse(
    await readFile(resolve(root, "scripts/architecture-policy.json"), "utf8"),
  );
  const pkg = JSON.parse(await readFile(resolve(root, "package.json"), "utf8"));
  const errors = [];
  for (const key of ["legacyCore", "legacyImports", "legacyUiSnippets"])
    if (Object.keys(policy[key] ?? {}).length)
      errors.push(`Legacy engine exceptions have been retired: ${key} must stay empty.`);
  for (const dependency of Object.keys({ ...pkg.dependencies, ...pkg.optionalDependencies })) {
    if (!policy.runtimeDependencies.includes(dependency))
      errors.push(`Runtime dependency ${dependency} is not permitted by the UI/Rust boundary.`);
  }
  const files = await applicationFiles(root);
  for (const file of files) {
    const source = (await readFile(resolve(root, file), "utf8")).replaceAll("\r\n", "\n");
    const bridge = file.startsWith("src/engine/");
    if (file !== "app.tsx" && !file.startsWith("src/ui/") && !bridge) {
      errors.push(
        `${file}: new JS/TS engine files are forbidden. Use src/ui/ for presentation, src/engine/ for the thin bridge, or crates/ for Rust engine code.`,
      );
      continue;
    }

    const checked = source;
    for (const match of checked.matchAll(pixelApi))
      errors.push(`${file}: pixel/codec API ${match[0]} belongs in Rust.`);
    if (!bridge)
      for (const match of checked.matchAll(pixelStorage))
        errors.push(
          `${file}: binary/pixel storage ${match[0]} is not UI state. Keep pixels in Rust and batching conversion in the bridge.`,
        );

    // Intentionally conservative: this is a lint guard, not a JavaScript parser or sandbox.
    // Non-literal imports cannot be checked against the architectural dependency graph.
    if (/\b(?:import|require)\s*\(\s*(?!\s|["'`])/.test(source))
      errors.push(
        `${file}: computed import/require is forbidden; use a statically reviewable native bridge.`,
      );
    const imports = /\b(?:from\s*|import\s*(?:\(\s*)?|require\s*\(\s*)(["'`])([^"'`]+)\1/g;
    for (const [, , specifier] of source.matchAll(imports)) {
      if (specifier.includes("${")) {
        errors.push(`${file}: computed module specifier ${specifier} is forbidden.`);
        continue;
      }
      if (specifier.startsWith(".")) {
        const target = posix.normalize(posix.join(posix.dirname(file), specifier));
        if (target.startsWith("src/core/")) {
          errors.push(`${file}: new legacy-engine import ${specifier}; use the Rust bridge.`);
          continue;
        }
        if (
          bridge
            ? target.startsWith("src/engine/")
            : target.startsWith("src/ui/") || target.startsWith("src/engine/")
        )
          continue;
        if (bridge && target.startsWith("native/") && target.endsWith(".node")) continue;
        if (target.startsWith("resources/") && /\.(?:png|jpg|jpeg|webp|svg)$/.test(target))
          continue;
        errors.push(`${file}: production import ${specifier} crosses the UI/engine boundary.`);
      } else if (!(bridge ? policy.bridgeImports : policy.uiImports).includes(specifier)) {
        errors.push(
          `${file}: import ${specifier} is not allowed here. Engine dependencies belong in Rust.`,
        );
      }
    }
  }
  return errors;
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
  try {
    const errors = await checkArchitecture(root);
    if (errors.length) {
      console.error(
        `Architecture check failed:\n${errors.map((error) => `- ${error}`).join("\n")}\nSee docs/architecture.md and AGENTS.md.`,
      );
      process.exitCode = 1;
    } else
      console.log(
        "Architecture check passed: TypeScript UI and native bridge only; no legacy engine exceptions.",
      );
  } catch (error) {
    console.error(error);
    process.exitCode = 1;
  }
}
