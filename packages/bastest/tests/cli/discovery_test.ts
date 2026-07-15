import { realpathSync } from "node:fs";
import { mkdir, mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { test } from "bastest";
import {
  assert,
  assertCliSnapshot,
  cliFixtures,
  normalizeLog,
  runCliIn,
} from "./helpers.ts";

test("CLI log normalization preserves temporary path delimiters", () => {
  const temporaryDirectory = path.join(
    realpathSync(tmpdir()),
    "bastest-cli-example",
  );
  const message =
    `could not find \`bastest.jsonc\` in \`${temporaryDirectory}\` or any parent directory`;
  assert(
    normalizeLog(message) ===
      "could not find `bastest.jsonc` in `<tmp>` or any parent directory",
  );
});

test("CLI discovers in-source tests by default", () => {
  const result = runCliIn(path.join(cliFixtures, "discovery", "in-source"));
  assert(result.status === 0, result.stderr);
  assertCliSnapshot(result);
});

test("CLI uses bastest.jsonc directory as project root", () => {
  const result = runCliIn(
    path.join(cliFixtures, "discovery", "project-root", "nested"),
  );
  assert(result.status === 0, result.stderr);
  assertCliSnapshot(result);
});

test("CLI fails when bastest.jsonc is not found", async () => {
  const dir = await mkdtemp(path.join(tmpdir(), "bastest-cli-"));
  try {
    await mkdir(path.join(dir, "src"), { recursive: true });
    await writeFile(
      path.join(dir, "src", "source.ts"),
      [
        'import { test } from "bastest";',
        'import { assert } from "bastest";',
        `if (import.meta${".test"}) {`,
        '  test("fallback source", () => assert(true));',
        "}",
      ].join("\n"),
      "utf8",
    );

    const result = runCliIn(dir);
    assert(result.status === 2);
    assertCliSnapshot(result);
  } finally {
    await rm(dir, { recursive: true, force: true });
  }
});

test("CLI shows help without bastest.jsonc", async () => {
  const dir = await mkdtemp(path.join(tmpdir(), "bastest-cli-"));
  try {
    const result = runCliIn(dir, "--help");
    assert(result.status === 0, result.stderr);
    assert(result.stdout.includes("Usage: bastest"));
  } finally {
    await rm(dir, { recursive: true, force: true });
  }
});

test("CLI can disable in-source test discovery", () => {
  const result = runCliIn(
    path.join(cliFixtures, "discovery", "in-source-disabled"),
  );
  assert(result.status === 0, result.stderr);
  assertCliSnapshot(result);
});

test("CLI excludes configured paths from discovery", () => {
  const result = runCliIn(path.join(cliFixtures, "discovery", "exclude"));
  assert(result.status === 0, result.stderr);
  assertCliSnapshot(result);
});

test("CLI excludes .git and node_modules by default", () => {
  const result = runCliIn(
    path.join(cliFixtures, "discovery", "default-excludes"),
  );
  assert(result.status === 0, result.stderr);
  assertCliSnapshot(result);
});

test("CLI config exclude overrides default excludes", () => {
  const result = runCliIn(
    path.join(cliFixtures, "discovery", "override-default-excludes"),
  );
  assert(result.status === 0, result.stderr);
  assertCliSnapshot(result);
});
