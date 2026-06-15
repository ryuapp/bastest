import { defineConfig } from "tsdown";

export default defineConfig({
  entry: {
    "runners/node": "src/runners/node.ts",
    index: "src/mod.ts",
  },
  format: "esm",
  platform: "node",
  target: "node24",
  dts: {
    tsgo: true,
  },
  clean: true,
  sourcemap: true,
});
