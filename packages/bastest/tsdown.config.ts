import { defineConfig } from "tsdown";

export default defineConfig({
  entry: {
    runner: "src/runner.ts",
    mod: "src/mod.ts",
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
