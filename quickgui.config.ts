import { defineConfig } from "@quickgui/cli";

export default defineConfig({
  language: "typescript",
  name: "Electropic",
  identifier: "dev.electropic.compositor",
  entry: "app.tsx",
  resources: ["THIRD_PARTY_NOTICES.md"],
});
