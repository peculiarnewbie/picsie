import test from "node:test";
import assert from "node:assert/strict";
import { mkdtemp, mkdir, writeFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { checkArchitecture } from "./check-architecture.mjs";

const legacySource = "export const oldEngine = 1;\n";
async function fixture(run) {
  const root = await mkdtemp(join(tmpdir(), "electropic-architecture-"));
  const put = async (file, source) => {
    await mkdir(dirname(join(root, file)), { recursive: true });
    await writeFile(join(root, file), source);
  };
  try {
    await put("package.json", JSON.stringify({ dependencies: { "solid-js": "2" } }));
    await put(
      "scripts/architecture-policy.json",
      JSON.stringify({
        runtimeDependencies: ["solid-js"],
        uiImports: ["solid-js"],
        bridgeImports: ["@electropic/engine"],
        legacyCore: {},
        legacyImports: {},
        legacyUiSnippets: {},
      }),
    );
    await put("app.tsx", 'import { engine } from "./src/engine/index.ts";\n');
    await run({ root, put, check: () => checkArchitecture(root) });
  } finally {
    await rm(root, { recursive: true, force: true });
  }
}

test("architecture permits UI and a thin native bridge", () =>
  fixture(async ({ put, check }) => {
    await put(
      "src/ui/panel.tsx",
      'import { createSignal } from "solid-js"; import { engine } from "../engine/index.ts";',
    );
    await put("src/engine/index.ts", 'export { engine } from "@electropic/engine";');
    assert.deepEqual(await check(), []);
  }));

test("architecture rejects new or modified TypeScript engine implementations", () =>
  fixture(async ({ put, check }) => {
    await put("src/core/old.ts", `${legacySource}export const invertMask = () => {};`);
    await put("src/core/new.ts", "export const mask = 1;");
    const errors = await check();
    assert.ok(errors.some((error) => error.includes("src/core/old.ts")));
    assert.ok(errors.some((error) => error.includes("new JS/TS engine files are forbidden")));
  }));

test("architecture rejects raster dependencies and production imports from tooling", () =>
  fixture(async ({ put, check }) => {
    await put("package.json", JSON.stringify({ optionalDependencies: { pngjs: "7" } }));
    await put("src/ui/panel.tsx", 'export { PNG } from "pngjs"; import "../../scripts/raster.ts";');
    const errors = await check();
    assert.ok(errors.some((error) => error.includes("Runtime dependency pngjs")));
    assert.ok(errors.some((error) => error.includes("import pngjs")));
    assert.ok(errors.some((error) => error.includes("production import ../../scripts/raster.ts")));
  }));

test("architecture catches pixel APIs, duplicate exceptions, and new legacy imports", () =>
  fixture(async ({ put, check }) => {
    await put("app.tsx", "canvas.getImageData(0, 0, 1, 1); canvas.getImageData(0, 0, 1, 1);");
    await put(
      "src/ui/panel.tsx",
      'import { oldEngine } from "../core/old.ts"; new Uint8Array(128);',
    );
    const errors = await check();
    assert.ok(errors.some((error) => error.includes("pixel/codec API getImageData")));
    assert.ok(errors.some((error) => error.includes("new legacy-engine import")));
    assert.ok(errors.some((error) => error.includes("binary/pixel storage Uint8Array")));
  }));

test("architecture catches computed imports and does not exempt bridge code from pixel algorithms", () =>
  fixture(async ({ put, check }) => {
    await put("src/ui/panel.tsx", "import(moduleName); require(`png${suffix}`);");
    await put(
      "src/engine/index.ts",
      'import "../ui/panel.tsx"; surface.putImageData(pixels, 0, 0);',
    );
    const errors = await check();
    assert.ok(errors.some((error) => error.includes("computed import/require")));
    assert.ok(errors.some((error) => error.includes("computed module specifier")));
    assert.ok(errors.some((error) => error.includes("pixel/codec API putImageData")));
    assert.ok(errors.some((error) => error.includes("production import ../ui/panel.tsx")));
  }));

test("application subfolders named scripts or tests cannot hide runtime pixel code", () =>
  fixture(async ({ put, check }) => {
    await put("src/ui/tests/pixels.ts", "new Uint8Array(256);");
    await put("src/ui/scripts/pixels.ts", "canvas.getImageData(0, 0, 1, 1);");
    const errors = await check();
    assert.ok(errors.some((error) => error.includes("src/ui/tests/pixels.ts")));
    assert.ok(errors.some((error) => error.includes("src/ui/scripts/pixels.ts")));
  }));

test("retired exception policies cannot authorize a TypeScript engine", () =>
  fixture(async ({ put, check }) => {
    await put(
      "scripts/architecture-policy.json",
      JSON.stringify({
        runtimeDependencies: ["solid-js"],
        uiImports: ["solid-js"],
        bridgeImports: [],
        legacyCore: { "src/core/old.ts": { sha256: "anything" } },
        legacyImports: { "app.tsx": ["./src/core/old.ts"] },
        legacyUiSnippets: { "app.tsx": [{ source: "canvas.getImageData(0,0,1,1)" }] },
      }),
    );
    await put("src/core/old.ts", legacySource);
    await put(
      "app.tsx",
      'import { oldEngine } from "./src/core/old.ts"; canvas.getImageData(0,0,1,1);',
    );
    const errors = await check();
    assert.ok(errors.some((error) => error.includes("exceptions have been retired")));
    assert.ok(errors.some((error) => error.includes("new JS/TS engine files")));
    assert.ok(errors.some((error) => error.includes("new legacy-engine import")));
    assert.ok(errors.some((error) => error.includes("pixel/codec API")));
  }));
