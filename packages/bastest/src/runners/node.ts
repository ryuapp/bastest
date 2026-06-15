#!/usr/bin/env node
import { createInterface } from "node:readline/promises";
import process from "node:process";
import { parseArgs, RESULT_PREFIX, runFile } from "./common.ts";
import type { RunnerOptions, WorkerRequest } from "./types.ts";

const options = parseArgs(process.argv.slice(2));
if (options.worker) {
  await runWorker(options);
} else {
  if (!options.file) {
    throw new Error("missing test file");
  }
  if (!options.bundleFile) {
    throw new Error("missing transformed test file");
  }

  const result = await runFile({
    ...options,
    file: options.file,
    bundleFile: options.bundleFile,
  });
  console.log(`${RESULT_PREFIX}${JSON.stringify(result)}`);
}

async function runWorker(options: RunnerOptions): Promise<void> {
  const lines = createInterface({
    input: process.stdin,
    crlfDelay: Infinity,
  });

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
      filter: request.filter ?? options.filter,
      stream: request.stream ?? true,
    });
    console.log(`${RESULT_PREFIX}${JSON.stringify(result)}`);
  }
}
