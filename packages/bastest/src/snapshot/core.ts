import type {
  AssertSnapshotMetadata,
  AssertSnapshotOptions,
  SnapshotContext,
} from "./context.ts";
import type { RuntimePlugin } from "../runtime/plugin.ts";

export function assertSnapshotWithContext(
  context: SnapshotContext,
  runtime: RuntimePlugin,
  value: unknown,
  options?: string | AssertSnapshotOptions,
  metadata?: AssertSnapshotMetadata,
): void {
  const name = typeof options === "string" ? options : options?.name;
  const expression = metadata?.expression ??
    (typeof options === "object" ? options.expression : undefined);
  const snapshotPath = resolveSnapshotPath({
    testFile: context.file,
    testName: context.testName,
    testNameOccurrence: context.testNameOccurrence,
    assertionIndex: context.index++,
    snapshotName: name,
  });
  const pendingPath = `${snapshotPath}.new`;
  const actual = serializeSnapshot(value);
  const expectedFile = runtime.readTextFile(snapshotPath);
  const expected = expectedFile === undefined
    ? undefined
    : parseSnapshotBody(expectedFile);

  if (expected === actual) {
    runtime.removeFile(pendingPath);
    return;
  }

  if (!isContinuousIntegration(runtime)) {
    runtime.createDir(parentDirectory(pendingPath));
    runtime.writeTextFile(
      pendingPath,
      formatSnapshotFile(context, actual, expression),
    );
  }

  const pendingMessage = isContinuousIntegration(runtime)
    ? "snapshot was not written because CI is set"
    : `pending: ${formatDisplayPath(pendingPath)}`;
  throw new SnapshotAssertionError({
    message: [
      "snapshot mismatch",
      `snapshot: ${formatDisplayPath(snapshotPath)}`,
      pendingMessage,
    ].join("\n"),
    actual,
    expected: expected ?? "",
    operator: "snapshot",
  });
}

function isContinuousIntegration(runtime: RuntimePlugin): boolean {
  return Boolean(runtime.getEnv("CI"));
}

export interface SnapshotPathInput {
  testFile: string;
  testName: string;
  testNameOccurrence: number;
  assertionIndex: number;
  snapshotName?: string;
}

export function resolveSnapshotPath(input: SnapshotPathInput): string {
  const dir = parentDirectory(input.testFile);
  const fileName = fileStem(input.testFile);
  const snapshotName = input.snapshotName ??
    (input.assertionIndex === 0 ? "" : String(input.assertionIndex + 1));
  const parts = [
    sanitize(fileName),
    sanitize(input.testName),
    input.testNameOccurrence > 1 ? String(input.testNameOccurrence) : undefined,
    snapshotName ? sanitize(snapshotName) : undefined,
  ].filter((part): part is string => Boolean(part));

  return joinPath(dir, "snapshots", `${parts.join("__")}.snap`);
}

export function relativePath(from: string, to: string): string {
  const normalizedFrom = trimTrailingSlash(formatDisplayPath(from));
  const normalizedTo = formatDisplayPath(to);
  const compareFrom = isWindowsPath(normalizedFrom)
    ? normalizedFrom.toLowerCase()
    : normalizedFrom;
  const compareTo = isWindowsPath(normalizedTo)
    ? normalizedTo.toLowerCase()
    : normalizedTo;
  if (compareTo === compareFrom) {
    return "";
  }
  if (compareTo.startsWith(`${compareFrom}/`)) {
    return normalizedTo.slice(normalizedFrom.length + 1);
  }
  return normalizedTo;
}

export function formatDisplayPath(path: string): string {
  return path.replaceAll("\\", "/");
}

class SnapshotAssertionError extends Error {
  readonly actual?: unknown;
  readonly expected?: unknown;
  readonly operator?: string;

  constructor(options: {
    message: string;
    actual?: unknown;
    expected?: unknown;
    operator?: string;
  }) {
    super(options.message);
    this.name = "AssertionError";
    this.actual = options.actual;
    this.expected = options.expected;
    this.operator = options.operator;
  }
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
    return formatDisplayPath(context.file);
  }

  return relativePath(context.cwd, context.file);
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

function parentDirectory(path: string): string {
  const slash = Math.max(path.lastIndexOf("/"), path.lastIndexOf("\\"));
  return slash === -1 ? "" : path.slice(0, slash);
}

function fileStem(path: string): string {
  const slash = Math.max(path.lastIndexOf("/"), path.lastIndexOf("\\"));
  const base = slash === -1 ? path : path.slice(slash + 1);
  const dot = base.lastIndexOf(".");
  return dot === -1 ? base : base.slice(0, dot);
}

function joinPath(base: string, ...parts: string[]): string {
  const separator = base.includes("\\") ? "\\" : "/";
  const prefix = trimTrailingSeparators(base);
  return [prefix, ...parts.map(trimSeparators)].filter(Boolean).join(separator);
}

function sanitize(value: string): string {
  return value
    .trim()
    .toLowerCase()
    .replace(/[^A-Za-z0-9._-]+/g, "_")
    .replace(/_+/g, "_")
    .replace(/^_+|_+$/g, "");
}

function trimTrailingSlash(path: string): string {
  return path.replace(/\/+$/g, "");
}

function trimTrailingSeparators(path: string): string {
  return path.replace(/[\\/]+$/g, "");
}

function trimSeparators(path: string): string {
  return path.replace(/^[\\/]+|[\\/]+$/g, "");
}

function isWindowsPath(path: string): boolean {
  return /^[a-z]:\//i.test(path);
}
