import { spawnSync } from "node:child_process";
import { rmSync } from "node:fs";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";

interface BenchCase {
  name: string;
  command: string;
  cwd: string;
  args: string[];
  beforeEach?: () => void;
  afterEach?: () => void;
}

const root = path.resolve(fileURLToPath(new URL("../..", import.meta.url)));
const benchmarkRoot = path.join(root, "benchmarks", "initial-run");
const vitestCache = path.join(root, "node_modules", ".vite", "vitest");
const bastestCore = path.join(
  root,
  "target",
  "release",
  process.platform === "win32" ? "bastest.exe" : "bastest",
);

const cases: BenchCase[] = [
  {
    name: "deno test",
    command: "deno",
    cwd: benchmarkRoot,
    args: ["task", "deno-test"],
  },
  {
    name: "node:test",
    command: "deno",
    cwd: benchmarkRoot,
    args: ["task", "node-test"],
  },
  {
    name: "vitest",
    command: "deno",
    cwd: benchmarkRoot,
    beforeEach: clearVitestCache,
    afterEach: clearVitestCache,
    args: ["task", "vitest"],
  },
  {
    name: "bastest",
    command: "deno",
    cwd: benchmarkRoot,
    args: ["task", "bastest"],
  },
  {
    name: "bastest-core",
    command: bastestCore,
    cwd: benchmarkRoot,
    args: ["cases/bastest/basic_test.ts"],
  },
];

for (const benchCase of cases) {
  Deno.bench(benchCase.name, () => {
    runOrThrow(benchCase);
  });
}

function runOrThrow(benchCase: BenchCase): void {
  benchCase.beforeEach?.();
  const child = spawnSync(benchCase.command, benchCase.args, {
    cwd: benchCase.cwd,
    encoding: "utf8",
    env: {
      ...process.env,
      NO_COLOR: "1",
    },
    maxBuffer: 1024 * 1024 * 64,
  });
  benchCase.afterEach?.();

  if (child.status !== 0) {
    process.stdout.write(child.stdout ?? "");
    process.stderr.write(child.stderr ?? "");
    throw new Error(`${benchCase.name} exited with ${child.status}`);
  }
}

function clearVitestCache(): void {
  rmSync(vitestCache, { recursive: true, force: true });
}
