import { readFileSync } from "node:fs";
import { defineConfig } from "@quickgui/cli";

// One version source: releases tag package.json's version as `v<version>`.
const { version } = JSON.parse(readFileSync(new URL("./package.json", import.meta.url), "utf8")) as {
  version: string;
};

export default defineConfig({
  language: "typescript",
  name: "Picsie",
  identifier: "dev.peculiarnewbie.picsie",
  version,
  entry: "app.tsx",
  resources: ["THIRD_PARTY_NOTICES.md"],
  windows: {
    publisher: "Picsie",
    description: "Picsie image editor",
  },
  updates: {
    // `quickgui build --upload` publishes a draft GitHub release as `v<version>`; the signed
    // appcast and install.sh/latest pointers follow the release once it is published.
    target: "github",
    github: { repository: "peculiarnewbie/picsie", tagPrefix: "v" },
    // `npx quickgui keygen` signs with QUICKGUI_UPDATER_PRIVATE_KEY in CI; this is its public key.
    publicKey: "pqBDdQsgvH2vl5K+SI2RPLiedvG2WppYwrI+Eso1Fm4=",
  },
});
