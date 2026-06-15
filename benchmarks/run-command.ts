import { spawnSync } from "node:child_process";
import path from "node:path";
import process from "node:process";

const [cwd, command, ...args] = process.argv.slice(2);

if (!cwd || !command) {
  console.error(
    "usage: deno run benchmarks/run-command.ts <cwd> <command> [...args]",
  );
  process.exit(2);
}

const result = spawnSync(command, args, {
  cwd: path.resolve(cwd),
  stdio: "inherit",
  env: process.env,
});

process.exit(result.status ?? 1);
