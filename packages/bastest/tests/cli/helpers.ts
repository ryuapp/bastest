import { spawnSync } from "node:child_process";
import { rmSync } from "node:fs";
import { readFile } from "node:fs/promises";
import path from "node:path";
import { assert, assertSnapshot } from "bastest";

export const packageRoot = process.cwd();
export const fixtures = path.join(packageRoot, "tests", "fixtures");
export const cliFixtures = path.join(fixtures, "cli");

const cli = requireEnv("BASTEST_CLI_PATH");

export interface CliResult {
  status: number | null;
  stdout: string;
  stderr: string;
}

export function runCli(...args: string[]): CliResult {
  return runCliIn(packageRoot, ...args);
}

export function fixtureArg(...segments: string[]): string {
  return path.posix.join("tests", "fixtures", ...segments);
}

export function runCliWithEnv(
  cwd: string,
  env: Record<string, string>,
  ...args: string[]
): CliResult {
  return runCliIn(cwd, ...args, env);
}

export function runCliIn(
  cwd: string,
  ...argsAndEnv: Array<string | Record<string, string>>
): CliResult {
  if (isInside(cwd, cliFixtures)) {
    rmSync(path.join(cwd, ".bastest"), { recursive: true, force: true });
  }

  const env = typeof argsAndEnv.at(-1) === "object"
    ? (argsAndEnv.pop() as Record<string, string>)
    : {};
  const args = argsAndEnv as string[];
  const processEnv = { ...process.env };
  delete processEnv.AI_AGENT;
  delete processEnv.CODEX_THREAD_ID;
  delete processEnv.CLAUDECODE;
  delete processEnv.CI;

  const result = spawnSync(cli, args, {
    cwd,
    encoding: "utf8",
    env: {
      ...processEnv,
      NO_COLOR: "1",
      BASTEST_RUNTIME_PATH: process.execPath,
      BASTEST_PACKAGE_ROOT: packageRoot,
      ...env,
    },
  });

  return {
    status: result.status,
    stdout: result.stdout ?? "",
    stderr: result.stderr ?? "",
  };
}

export function assertCliSnapshot(result: CliResult): void {
  assertSnapshot([
    `status: ${result.status}`,
    "stdout:",
    indentBlock(normalizeLog(result.stdout)),
    "stderr:",
    indentBlock(normalizeLog(result.stderr)),
  ].join("\n"));
}

export async function exists(file: string): Promise<boolean> {
  try {
    await readFile(file, "utf8");
    return true;
  } catch (error) {
    if (error && typeof error === "object" && "code" in error) {
      return (error as { code?: unknown }).code !== "ENOENT";
    }
    throw error;
  }
}

function requireEnv(name: string): string {
  const value = process.env[name];
  if (!value) {
    throw new Error(`${name} is not set`);
  }
  return value;
}

function indentBlock(value: string): string {
  if (!value) {
    return "  <empty>";
  }
  return value.split("\n").map((line) => `  ${line}`).join("\n");
}

export function normalizeLog(value: string): string {
  return normalizeNewlines(value)
    .replace(new RegExp(pathPattern(fixtures), "gi"), "tests/fixtures")
    .replace(
      /[A-Z]:[\\/][^ \n"`]*[\\/](?:Temp|tmp)[\\/]bastest-cli-[^ \n"`]+/gi,
      "<tmp>",
    )
    .replace(/\/tmp\/bastest-cli-[^ \n"`]+/g, "<tmp>")
    .replace(
      /file:\/\/\/[^ \n"]+\.(?:ts|mjs|js|cjs|mts)/g,
      "<module-url>",
    )
    .replace(/(?<![\w.])\d+(?:\.\d+)?ms/g, "<duration>")
    .replace(/(?<![\w.])\d+(?:\.\d+)?s/g, "<duration>")
    .replace(/^[ \t]+at .*$/gm, "    <stack>")
    .trimEnd();
}

function normalizeNewlines(value: string): string {
  return value.replace(/\r\n/g, "\n");
}

function isInside(file: string, dir: string): boolean {
  const relative = path.relative(dir, file);
  return relative === "" ||
    (!relative.startsWith("..") && !path.isAbsolute(relative));
}

function pathPattern(file: string): string {
  return escapeRegExp(path.resolve(file)).replace(/[\\/]+/g, String.raw`[\\/]`);
}

function escapeRegExp(value: string): string {
  return value.replace(/[.*+?^${}()|[\]\\]/g, String.raw`\$&`);
}

export { assert };
