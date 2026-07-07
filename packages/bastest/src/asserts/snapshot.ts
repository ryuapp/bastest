import { mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import path from "node:path";
import { AssertionError } from "./error.ts";

export interface AssertSnapshotOptions {
  name?: string;
  expression?: string;
}

export interface AssertSnapshotMetadata {
  expression?: string;
}

export function assertSnapshot(
  value: unknown,
  options?: string | AssertSnapshotOptions,
  metadata?: AssertSnapshotMetadata,
): void {
  const context = globalThis.__bastest_snapshot;
  if (!context) {
    throw new Error("assertSnapshot must be called inside a bastest test");
  }

  const name = typeof options === "string" ? options : options?.name;
  const expression = metadata?.expression ??
    (typeof options === "object" ? options.expression : undefined);
  const snapshotPath = snapshotFilePath(context, name);
  const pendingPath = `${snapshotPath}.new`;
  const actual = serializeSnapshot(value);
  const expectedFile = readText(snapshotPath);
  const expected = expectedFile === undefined
    ? undefined
    : parseSnapshotBody(expectedFile);

  if (expected === actual) {
    rmSync(pendingPath, { force: true });
    return;
  }

  if (!isCi()) {
    mkdirSync(path.dirname(snapshotPath), { recursive: true });
    writeFileSync(
      pendingPath,
      formatSnapshotFile(context, actual, expression),
      "utf8",
    );
  }

  const pendingMessage = isCi()
    ? "snapshot was not written because CI is set"
    : `pending: ${displayPath(pendingPath)}`;
  throw new AssertionError({
    message: [
      "snapshot mismatch",
      `snapshot: ${displayPath(snapshotPath)}`,
      pendingMessage,
    ].join("\n"),
    actual,
    expected: expected ?? "",
    operator: "snapshot",
  });
}

function isCi(): boolean {
  return Boolean(process.env.CI);
}

function snapshotFilePath(
  context: SnapshotContext,
  name: string | undefined,
): string {
  const testFile = path.parse(context.file);
  const index = context.index++;
  const snapshotName = name ?? (index === 0 ? "" : String(index + 1));
  const parts = [
    sanitize(testFile.name),
    sanitize(context.testName),
    context.testNameOccurrence > 1
      ? String(context.testNameOccurrence)
      : undefined,
    snapshotName ? sanitize(snapshotName) : undefined,
  ].filter((part): part is string => Boolean(part));

  return path.join(testFile.dir, "snapshots", `${parts.join("__")}.snap`);
}

function serializeSnapshot(value: unknown): string {
  if (typeof value === "string") {
    return value.endsWith("\n") ? value : `${value}\n`;
  }

  const serialized = JSON.stringify(value, null, 2);
  return `${serialized ?? String(value)}\n`;
}

function formatSnapshotFile(
  context: SnapshotContext,
  body: string,
  expression: string | undefined,
): string {
  const metadata = [
    "---",
    `source: ${snapshotSource(context)}`,
    expression ? `expression: ${expression}` : undefined,
    "---",
  ].filter((line): line is string => Boolean(line));

  return [
    ...metadata,
    body,
  ].join("\n");
}

function snapshotSource(context: SnapshotContext): string {
  if (!context.cwd) {
    return displayPath(context.file);
  }

  return displayPath(path.relative(context.cwd, context.file));
}

function parseSnapshotBody(file: string): string {
  const normalized = file.replace(/\r\n/g, "\n");
  if (!normalized.startsWith("---\n")) {
    return normalized;
  }

  const bodyStart = normalized.indexOf("\n---\n", "---\n".length);
  if (bodyStart === -1) {
    return normalized;
  }

  return normalized.slice(bodyStart + "\n---\n".length);
}

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

function sanitize(value: string): string {
  return value
    .trim()
    .toLowerCase()
    .replace(/[^A-Za-z0-9._-]+/g, "_")
    .replace(/_+/g, "_")
    .replace(/^_+|_+$/g, "");
}

function displayPath(file: string): string {
  return file.replaceAll("\\", "/");
}

interface SnapshotContext {
  cwd?: string;
  file: string;
  testName: string;
  testNameOccurrence: number;
  index: number;
}
