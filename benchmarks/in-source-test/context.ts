import path from "node:path";
import { fileURLToPath } from "node:url";

export const root = path.resolve(
  fileURLToPath(new URL("../..", import.meta.url)),
);
export const benchmarkRoot = path.join(root, "benchmarks", "in-source-test");
export const tmp = path.join(benchmarkRoot, ".tmp");
export const bastestProject = path.join(tmp, "bastest");
export const vitestProject = path.join(tmp, "vitest");
export const vitestCache = path.join(root, "node_modules", ".vite", "vitest");

export const fileCount = 300;
export const inSourceEvery = 3;
