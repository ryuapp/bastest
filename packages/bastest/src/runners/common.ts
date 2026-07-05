import { createRegistry, serializeError } from "../runtime.ts";
import type { FileResult } from "../types.ts";
import type { RunFileOptions, RunnerOptions } from "./types.ts";

export const EVENT_PREFIX = "__BASTEST_EVENT__";
export const RESULT_PREFIX = "__BASTEST_RESULT__";

export function parseArgs(args: string[]): RunnerOptions {
  const options: RunnerOptions = {
    worker: false,
  };

  for (let index = 0; index < args.length; index++) {
    const arg = args[index];
    if (arg === "--worker") {
      options.worker = true;
    } else if (arg === "--cwd") {
      options.cwd = readValue(args, ++index, "--cwd");
    } else if (arg === "--file") {
      options.file = readValue(args, ++index, "--file");
    } else if (arg === "--bundle-file") {
      options.bundleFile = readValue(args, ++index, "--bundle-file");
    } else if (arg === "--filter") {
      options.filter = readValue(args, ++index, "--filter");
    } else if (arg.startsWith("--filter=")) {
      options.filter = arg.slice("--filter=".length);
    } else {
      options.file = arg;
    }
  }

  return options;
}

export function readValue(args: string[], index: number, flag: string): string {
  const value = args[index];
  if (!value) {
    throw new Error(`${flag} requires a value`);
  }
  return value;
}

export async function runFile(options: RunFileOptions): Promise<FileResult> {
  const registry = createRegistry();
  globalThis.__bastest_api = registry.api;
  globalThis.__bastest_current_file = options.file;
  globalThis.__bastest_current_cwd = options.cwd;

  const started = performance.now();
  try {
    await import(options.bundleFile);
  } catch (error) {
    globalThis.__bastest_api = undefined;
    globalThis.__bastest_current_file = undefined;
    globalThis.__bastest_current_cwd = undefined;
    return {
      file: options.file,
      durationMs: elapsed(started),
      tests: [],
      loadError: serializeError(error),
    };
  }

  const filter = options.filter ? new RegExp(options.filter) : undefined;
  try {
    const tests = await registry.runAll({
      filter,
      onTest: options.stream
        ? (test) => {
          console.log(
            `${EVENT_PREFIX}${
              JSON.stringify({ type: "test", file: options.file, test })
            }`,
          );
        }
        : undefined,
    });
    return {
      file: options.file,
      durationMs: elapsed(started),
      tests,
    };
  } finally {
    globalThis.__bastest_api = undefined;
    globalThis.__bastest_current_file = undefined;
    globalThis.__bastest_current_cwd = undefined;
  }
}

function elapsed(started: number): number {
  return Math.round((performance.now() - started) * 100) / 100;
}
