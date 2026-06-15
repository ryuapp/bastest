import { spawnSync } from "node:child_process";
import { rmSync } from "node:fs";
import process from "node:process";
import { vitestCache } from "./context.ts";
import type { BenchCase } from "./cases.ts";

export function runOrThrow(
  benchCase: BenchCase,
  bench?: Deno.BenchContext,
): void {
  benchCase.beforeEach?.();
  bench?.start();
  const child = spawnSync(benchCase.command, benchCase.args, {
    cwd: benchCase.cwd,
    encoding: "utf8",
    env: {
      ...process.env,
      NO_COLOR: "1",
    },
    maxBuffer: 1024 * 1024 * 64,
  });
  bench?.end();
  benchCase.afterEach?.();

  if (child.status !== 0) {
    process.stdout.write(child.stdout ?? "");
    process.stderr.write(child.stderr ?? "");
    throw new Error(`${benchCase.name} exited with ${child.status}`);
  }
}

export function clearVitestCache(): void {
  rmSync(vitestCache, { recursive: true, force: true });
}
