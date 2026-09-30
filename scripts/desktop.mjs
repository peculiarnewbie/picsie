import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { chmod, copyFile, cp, mkdir, readFile, readdir, rm, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const manifest = "crates/picsie-desktop/Cargo.toml";
const packagerVersion = "0.11.8";
const [action, ...args] = process.argv.slice(2);
process.chdir(root);

function run(command, arguments_, capture = false) {
  const result = spawnSync(command, arguments_, {
    stdio: capture ? "pipe" : "inherit",
    encoding: "utf8",
    maxBuffer: 32 * 1024 * 1024,
  });
  if (result.error) throw result.error;
  if (result.status !== 0) {
    if (capture) process.stderr.write(result.stderr);
    throw new Error(`${command} exited with ${result.status ?? result.signal}`);
  }
  return result.stdout?.trim();
}

async function collectLicenses(directory, targetTriple) {
  const metadata = JSON.parse(
    run(
      "cargo",
      [
        "metadata",
        "--locked",
        "--format-version",
        "1",
        "--filter-platform",
        targetTriple,
        "--manifest-path",
        manifest,
      ],
      true,
    ),
  );
  const records = [];
  for (const dependency of metadata.packages) {
    const source = path.dirname(dependency.manifest_path);
    const files = (await readdir(source)).filter((name) =>
      /^(license|licence|notice|copying)([._-]|$)/i.test(name),
    );
    if (dependency.license_file && !files.includes(dependency.license_file))
      files.push(dependency.license_file);
    for (const file of files) {
      const destination = path.join(
        directory,
        `${dependency.name}-${dependency.version}`,
        path.basename(file),
      );
      await mkdir(path.dirname(destination), { recursive: true });
      await cp(path.join(source, file), destination, { recursive: true });
    }
    records.push({
      name: dependency.name,
      version: dependency.version,
      license: dependency.license,
      repository: dependency.repository,
      files,
    });
  }
  await writeFile(
    path.join(directory, "dependencies.json"),
    JSON.stringify(records, null, 2) + "\n",
  );
}

async function packageDesktop() {
  if (args.length)
    throw new Error("Build on the target OS/architecture; npm run build takes no target override.");
  const packageInfo = JSON.parse(await readFile("package.json", "utf8"));
  const cargoInfo = JSON.parse(
    run(
      "cargo",
      ["metadata", "--locked", "--no-deps", "--format-version", "1", "--manifest-path", manifest],
      true,
    ),
  );
  const desktop = cargoInfo.packages.find((p) => p.name === "picsie-desktop");
  if (desktop.version !== packageInfo.version)
    throw new Error("package.json and picsie-desktop/Cargo.toml versions must match.");
  let packager;
  try {
    packager = run("cargo", ["packager", "--version"], true);
  } catch {
    throw new Error("Install the native packager with npm run setup:packager.");
  }
  if (packager !== `cargo-packager ${packagerVersion}`)
    throw new Error(`Run npm run setup:packager (requires cargo-packager ${packagerVersion}).`);
  const targetTriple = run("rustc", ["-vV"], true).match(/^host: (.+)$/m)[1];
  if (process.env.CARGO_BUILD_TARGET && process.env.CARGO_BUILD_TARGET !== targetTriple)
    throw new Error(
      "Native packaging must run on the target OS/architecture; clear CARGO_BUILD_TARGET.",
    );
  const platform = { linux: "linux", win32: "windows", darwin: "macos" }[process.platform];
  const architecture = { x86_64: "x64", aarch64: "arm64" }[targetTriple.split("-")[0]];
  if (!platform || !architecture) throw new Error("Unsupported packaging platform.");
  const target = `${platform}-${architecture}`;
  const output = path.join(root, "dist", target);
  const staging = path.join(root, "artifacts", "desktop-package", target);
  const portableName = `picsie-${packageInfo.version}-${target}`;
  const portable = path.join(staging, portableName);
  run("cargo", ["build", "--locked", "--release", "--manifest-path", manifest]);
  await rm(staging, { recursive: true, force: true });
  await rm(output, { recursive: true, force: true });
  await mkdir(portable, { recursive: true });
  await mkdir(output, { recursive: true });
  const executable = process.platform === "win32" ? "picsie.exe" : "picsie";
  const binary = path.join(portable, executable);
  await copyFile(
    path.join(
      cargoInfo.target_directory,
      "release",
      process.platform === "win32" ? "picsie-desktop.exe" : "picsie-desktop",
    ),
    binary,
  );
  if (process.platform !== "win32") await chmod(binary, 0o755);
  await copyFile("THIRD_PARTY_NOTICES.md", path.join(portable, "THIRD_PARTY_NOTICES.md"));
  await mkdir(path.join(portable, "licenses"));
  await collectLicenses(path.join(portable, "licenses"), targetTriple);
  await writeFile(
    path.join(portable, "README.txt"),
    `Picsie ${packageInfo.version}\n\nRun ${executable} to open the editor, or pass one or more .picsie/.electropic/.comp paths.\nAll editor assets are embedded; Node, Bun, and QuickGUI are not required.\n\nLinux needs a Vulkan driver, fontconfig, FreeType, X11/Wayland and libxkbcommon.\nFile dialogs need xdg-desktop-portal and a file chooser backend (for example xdg-desktop-portal-gtk).\nSee https://github.com/peculiarnewbie/picsie for setup and platform limits.\n`,
  );
  const config = {
    name: "picsie",
    productName: "Picsie",
    identifier: "dev.peculiarnewbie.picsie",
    version: packageInfo.version,
    publisher: "Picsie",
    category: "GraphicsAndDesign",
    description: "Image editor with a Rust engine and GPUI Kit interface",
    homepage: "https://github.com/peculiarnewbie/picsie",
    outDir: output,
    binariesDir: portable,
    binaries: [{ path: "picsie", main: true }],
    icons: [path.join(root, "resources/icon.png")],
    resources: [
      {
        src: path.join(portable, "THIRD_PARTY_NOTICES.md"),
        target: "picsie/THIRD_PARTY_NOTICES.md",
      },
      { src: path.join(portable, "licenses"), target: "picsie/licenses" },
    ],
    // Finder's open-document callback has not been validated yet; avoid registering macOS types.
    fileAssociations:
      process.platform === "darwin"
        ? []
        : [
            {
              extensions: ["picsie"],
              name: "Picsie project",
              description: "Picsie project",
              mimeType: "application/x-picsie",
            },
            {
              extensions: ["electropic"],
              name: "Legacy Electropic project",
              description: "Legacy Electropic project",
              mimeType: "application/x-electropic",
            },
          ],
    formats: { linux: ["deb"], windows: ["nsis"], macos: ["app", "dmg"] }[platform],
    deb: {
      section: "graphics",
      depends: [
        "libc6",
        "libstdc++6",
        "libfontconfig1",
        "libfreetype6",
        "libxcb1",
        "libwayland-client0",
        "libxkbcommon0",
        "libxkbcommon-x11-0",
        "libvulkan1",
        "xdg-desktop-portal",
      ],
      files: { [path.join(root, "resources/picsie.xml")]: "usr/share/mime/packages/picsie.xml" },
    },
    nsis: { installMode: "currentUser" },
  };
  const configPath = path.join(staging, "packager.json");
  await writeFile(configPath, JSON.stringify(config, null, 2) + "\n");
  run("cargo", ["packager", "--config", configPath]);
  await rm(path.join(output, ".cargo-packager"), { recursive: true, force: true });
  if (process.platform === "win32") {
    const escape = (value) => `'${value.replaceAll("'", "''")}'`;
    const command = `$ErrorActionPreference = 'Stop'; Compress-Archive -LiteralPath ${escape(portable)} -DestinationPath ${escape(path.join(output, `${portableName}.zip`))}`;
    run("powershell.exe", [
      "-NoProfile",
      "-NonInteractive",
      "-EncodedCommand",
      Buffer.from(command, "utf16le").toString("base64"),
    ]);
  } else if (process.platform === "linux") {
    run("tar", ["-czf", path.join(output, `${portableName}.tar.gz`), "-C", staging, portableName]);
  }
  const artifacts = (await readdir(output))
    .filter((name) => /\.(deb|exe|zip|tar\.gz|dmg)$/.test(name))
    .sort();
  if (!artifacts.length) throw new Error("Packaging produced no release artifacts.");
  for (const extension of {
    linux: [".deb", ".tar.gz"],
    windows: [".exe", ".zip"],
    macos: [".dmg"],
  }[platform])
    if (artifacts.filter((name) => name.endsWith(extension)).length !== 1)
      throw new Error(`Expected one ${extension} release artifact.`);
  const checksums = [];
  for (const name of artifacts)
    checksums.push(
      `${createHash("sha256")
        .update(await readFile(path.join(output, name)))
        .digest("hex")}  ${name}`,
    );
  await writeFile(path.join(output, `SHA256SUMS-${target}.txt`), checksums.join("\n") + "\n");
  console.log(`Picsie packages: ${path.relative(root, output)}`);
}

try {
  run(process.execPath, ["scripts/check-architecture.mjs"]);
  if (action === "dev")
    run("cargo", ["run", "--locked", "--release", "--manifest-path", manifest, "--", ...args]);
  else if (action === "build")
    run("cargo", ["build", "--locked", "--release", "--manifest-path", manifest, ...args]);
  else if (action === "package") await packageDesktop();
  else throw new Error("Usage: node scripts/desktop.mjs dev|build|package");
} catch (error) {
  console.error(error.message);
  process.exitCode = 1;
}
