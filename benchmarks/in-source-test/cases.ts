import { benchmarkRoot } from "./context.ts";
import { clearVitestCache } from "./runner.ts";

export interface BenchCase {
  name: string;
  command: string;
  cwd: string;
  args: string[];
  beforeEach?: () => void;
  afterEach?: () => void;
}

export const cases: BenchCase[] = [
  {
    name: "vitest includeSource",
    command: "deno",
    cwd: benchmarkRoot,
    beforeEach: clearVitestCache,
    afterEach: clearVitestCache,
    args: ["task", "vitest"],
  },
  {
    name: "bastest inSourceTest",
    command: "deno",
    cwd: benchmarkRoot,
    args: ["task", "bastest"],
  },
  {
    name: "bastest-core inSourceTest",
    command: "deno",
    cwd: benchmarkRoot,
    args: ["task", "bastest-core"],
  },
];
