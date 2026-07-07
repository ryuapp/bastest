import { createInterface } from "node:readline/promises";
import { mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import process from "node:process";
import { defineRuntimePlugin } from "./plugin.ts";

const runtimePlugin = defineRuntimePlugin({
  now() {
    return performance.now();
  },

  log(message) {
    console.log(message);
  },

  args() {
    return process.argv.slice(2);
  },

  readLines() {
    return createInterface({
      input: process.stdin,
      crlfDelay: Infinity,
    });
  },

  readTextFile(path) {
    return readText(path);
  },

  createDir(path) {
    mkdirSync(path, { recursive: true });
  },

  writeTextFile(path, content) {
    writeFileSync(path, content, "utf8");
  },

  removeFile(path) {
    rmSync(path, { force: true });
  },

  getEnv(name) {
    return process.env[name];
  },
});

export default runtimePlugin;

function readText(file: string): string | undefined {
  try {
    return readFileSync(file, "utf8");
  } catch (error) {
    if (error && typeof error === "object" && "code" in error) {
      if ((error as { code?: unknown }).code === "ENOENT") {
        return undefined;
      }
    }
    throw error;
  }
}
