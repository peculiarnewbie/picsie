#!/usr/bin/env node
/**
 * Fix @quickgui/cli 0.1.6's NSIS generator for Windows releases.
 *
 * `nsisScript` emits `InstallDirRegKey SHCTX ...`, but NSIS's InstallDirRegKey only accepts
 * HKCR/HKLM/HKCU/HKU/HKCC/HKDD/HKPD as its root key (`SHCTX` works in WriteReg* only), so
 * makensis aborts on line 12 of the generated script. Use the registry hive that matches the
 * install scope instead. Remove this file once the upstream fix ships in a pinned QuickGUI.
 */
import { readFileSync, writeFileSync } from "node:fs";

const target = new URL("../node_modules/@quickgui/cli/src/packaging/windows.ts", import.meta.url);
const broken = "InstallDirRegKey SHCTX";
const fixed = 'InstallDirRegKey ${options.perMachine ? "HKLM" : "HKCU"}';
let source;
try {
  source = readFileSync(target, "utf8");
} catch {
  console.log("patch-quickgui-nsis: @quickgui/cli is not installed; nothing to patch");
  process.exit(0);
}
if (source.includes(fixed)) {
  process.exit(0);
}
if (!source.includes(broken)) {
  console.error(
    "patch-quickgui-nsis: @quickgui/cli's NSIS generator changed; review this patch before releasing",
  );
  process.exit(1);
}
writeFileSync(target, source.replaceAll(broken, fixed));
console.log("patch-quickgui-nsis: patched InstallDirRegKey for @quickgui/cli 0.1.6");
