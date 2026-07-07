import { createRegistry, serializeError } from "./runtime.ts";
import runtimePlugin from "./runtime/node-plugin.ts";
import type { RuntimePlugin } from "./runtime/plugin.ts";
import type { FileResult } from "./types.ts";

export const EVENT_PREFIX = "__BASTEST_EVENT__";
export const RESULT_PREFIX = "__BASTEST_RESULT__";

export interface RunnerOptions {
  worker: boolean;
  cwd?: string;
  file?: string;
  bundleFile?: string;
  filter?: string;
}

export interface RunFileOptions {
  runtime: RuntimePlugin;
  cwd?: string;
  file: string;
  bundleFile: string;
  filter?: string;
  stream?: boolean;
}

export interface WorkerRequest extends RunnerOptions {
  file: string;
  bundleFile: string;
  stream?: boolean;
}

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

  const started = options.runtime.now();
  try {
    await import(options.bundleFile);
  } catch (error) {
    globalThis.__bastest_api = undefined;
    globalThis.__bastest_current_file = undefined;
    globalThis.__bastest_current_cwd = undefined;
    return {
      file: options.file,
      durationMs: elapsed(options.runtime, started),
      tests: [],
      loadError: serializeError(error),
    };
  }

  const filter = options.filter ? new RegExp(options.filter) : undefined;
  try {
    const tests = await registry.runAll({
      runtime: options.runtime,
      filter,
      onTest: options.stream
        ? (test) => {
          options.runtime.log(
            `${EVENT_PREFIX}${
              JSON.stringify({ type: "test", file: options.file, test })
            }`,
          );
        }
        : undefined,
    });
    return {
      file: options.file,
      durationMs: elapsed(options.runtime, started),
      tests,
    };
  } finally {
    globalThis.__bastest_api = undefined;
    globalThis.__bastest_current_file = undefined;
    globalThis.__bastest_current_cwd = undefined;
  }
}

function elapsed(
  runtime: RuntimePlugin,
  started: number,
): number {
  return Math.round((runtime.now() - started) * 100) / 100;
}

export async function runRunner(runtime: RuntimePlugin): Promise<void> {
  const options = parseArgs(runtime.args());
  if (options.worker) {
    await runWorker(runtime, options);
    return;
  }

  if (!options.file) {
    throw new Error("missing test file");
  }
  if (!options.bundleFile) {
    throw new Error("missing transformed test file");
  }

  const result = await runFile({
    ...options,
    runtime,
    file: options.file,
    bundleFile: options.bundleFile,
  });
  runtime.log(`${RESULT_PREFIX}${JSON.stringify(result)}`);
}

async function runWorker(
  runtime: RuntimePlugin,
  options: RunnerOptions,
): Promise<void> {
  const lines = runtime.readLines();

  for await (const line of lines) {
    if (!line.trim()) {
      continue;
    }
    if (line === "__BASTEST_SHUTDOWN__") {
      lines.close();
      break;
    }
    const request = JSON.parse(line) as WorkerRequest;
    const result = await runFile({
      ...options,
      ...request,
      runtime,
      filter: request.filter ?? options.filter,
      stream: request.stream ?? true,
    });
    runtime.log(`${RESULT_PREFIX}${JSON.stringify(result)}`);
  }
}

await runRunner(runtimePlugin);
