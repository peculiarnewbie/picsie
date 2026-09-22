import { defineConfig } from "@quickgui/cli";

export default defineConfig({
  language: "typescript",
  name: "Picsie",
  identifier: "dev.peculiarnewbie.picsie",
  entry: "app.tsx",
  resources: ["THIRD_PARTY_NOTICES.md"],
});
