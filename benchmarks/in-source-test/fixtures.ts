import { mkdirSync, rmSync, writeFileSync } from "node:fs";
import path from "node:path";
import {
  bastestProject,
  fileCount,
  inSourceEvery,
  tmp,
  vitestProject,
} from "./context.ts";

export function prepareFixture(): void {
  rmSync(tmp, { recursive: true, force: true });
  writeBastestProject();
  writeVitestProject();
}

function writeBastestProject(): void {
  mkdirSync(path.join(bastestProject, "src"), { recursive: true });
  writeFileSync(
    path.join(bastestProject, "bastest.jsonc"),
    '{\n  "inSourceTest": true\n}\n',
    "utf8",
  );
  for (let index = 0; index < fileCount; index++) {
    writeFileSync(
      path.join(bastestProject, "src", `file-${index}.ts`),
      index % inSourceEvery === 0 ? bastestSource(index) : plainSource(index),
      "utf8",
    );
  }
}

function writeVitestProject(): void {
  mkdirSync(path.join(vitestProject, "src"), { recursive: true });
  writeFileSync(
    path.join(vitestProject, "vitest.config.ts"),
    'export default { test: { includeSource: ["src/**/*.ts"] } };\n',
    "utf8",
  );
  for (let index = 0; index < fileCount; index++) {
    writeFileSync(
      path.join(vitestProject, "src", `file-${index}.ts`),
      index % inSourceEvery === 0 ? vitestSource(index) : plainSource(index),
      "utf8",
    );
  }
}

function plainSource(index: number): string {
  const lines = [
    `export function value${index}() {`,
    `  return ${index};`,
    "}",
    "",
  ];
  for (let line = 0; line < 296; line++) {
    lines.push(`export const value${index}_${line} = ${index + line};`);
  }
  return lines.join("\n");
}

function bastestSource(index: number): string {
  return [
    'import { test } from "bastest";',
    'import { assert } from "bastest";',
    "",
    ...plainSource(index).split("\n"),
    "if (import.meta.test) {",
    `  test("value ${index}", () => {`,
    `    assert(value${index}() === ${index});`,
    "  });",
    "}",
    "",
  ].join("\n");
}

function vitestSource(index: number): string {
  return [
    ...plainSource(index).split("\n"),
    "if (import.meta.vitest) {",
    "  const { test, expect } = import.meta.vitest;",
    `  test("value ${index}", () => {`,
    `    expect(value${index}()).toBe(${index});`,
    "  });",
    "}",
    "",
  ].join("\n");
}
