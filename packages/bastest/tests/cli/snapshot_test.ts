import { mkdir, readFile, rename, rm, writeFile } from "node:fs/promises";
import path from "node:path";
import { test } from "bastest";
import {
  assert,
  assertCliSnapshot,
  cliFixtures,
  exists,
  runCliIn,
  runCliWithEnv,
} from "./helpers.ts";

const snapshotSource = [
  'import { assertSnapshot, test } from "bastest";',
  "",
  'test("records value", () => {',
  '  assertSnapshot({ name: "bastest", marker: "import.meta.test", ok: true });',
  "});",
  'test("duplicate name", () => {',
  "  assertSnapshot({ duplicate: 1 });",
  "});",
  'test("duplicate name", () => {',
  "  assertSnapshot({ duplicate: 2 });",
  "});",
  "",
].join("\n");

const changedSnapshotSource = snapshotSource.replace("ok: true", "ok: false");

const crlfSnapshotSource = [
  'import { assertSnapshot, test } from "bastest";',
  "",
  'test("records value", () => {',
  '  assertSnapshot({ name: "bastest", ok: true });',
  "});",
  "",
].join("\n");

test("CLI writes pending snapshots and accepts them", async () => {
  const dir = path.join(cliFixtures, "snapshot", "pending");
  const source = path.join(dir, "src", "snapshot_test.ts");
  const snapshot = path.join(
    dir,
    "src",
    "snapshots",
    "snapshot_test__records_value.snap",
  );
  const pending = `${snapshot}.new`;
  try {
    await writeFile(source, snapshotSource, "utf8");
    await rm(path.join(dir, ".bastest"), { recursive: true, force: true });
    await rm(path.join(dir, "src", "snapshots"), {
      recursive: true,
      force: true,
    });

    const missing = runCliIn(dir);
    assert(missing.status === 1);
    assertCliSnapshot(missing);
    assert(
      await readFile(pending, "utf8") ===
        '---\nsource: src/snapshot_test.ts\nexpression: { name: "bastest", marker: "import.meta.test", ok: true }\n---\n{\n  "name": "bastest",\n  "marker": "import.meta.test",\n  "ok": true\n}\n',
    );
    assert(
      await exists(path.join(
        dir,
        "src",
        "snapshots",
        "snapshot_test__duplicate_name.snap.new",
      )),
    );
    assert(
      await exists(path.join(
        dir,
        "src",
        "snapshots",
        "snapshot_test__duplicate_name__2.snap.new",
      )),
    );

    const accept = runCliIn(dir, "snapshot", "accept");
    assert(accept.status === 0, accept.stdout + accept.stderr);
    assertCliSnapshot(accept);
    const acceptedSnapshot = await readFile(snapshot, "utf8");
    assert(acceptedSnapshot.includes('"bastest"'));

    const matched = runCliIn(dir);
    assert(matched.status === 0, matched.stdout + matched.stderr);
    assertCliSnapshot(matched);

    await writeFile(source, changedSnapshotSource, "utf8");

    const changed = runCliIn(dir);
    assert(changed.status === 1);
    assertCliSnapshot(changed);
    const changedSnapshot = await readFile(pending, "utf8");
    assert(changedSnapshot.includes('"ok": false'));

    await rename(pending, snapshot);
    const updated = runCliIn(dir);
    assert(updated.status === 0, updated.stdout + updated.stderr);
    assertCliSnapshot(updated);
  } finally {
    await writeFile(source, snapshotSource, "utf8");
    await rm(path.join(dir, ".bastest"), { recursive: true, force: true });
    await rm(path.join(dir, "src", "snapshots"), {
      recursive: true,
      force: true,
    });
  }
});

test("CLI does not write pending snapshots in CI", async () => {
  const dir = path.join(cliFixtures, "snapshot", "ci");
  const pending = path.join(
    dir,
    "src",
    "snapshots",
    "snapshot_test__records_value.snap.new",
  );
  try {
    await rm(path.join(dir, ".bastest"), { recursive: true, force: true });
    await rm(path.join(dir, "src", "snapshots"), {
      recursive: true,
      force: true,
    });

    const result = runCliWithEnv(dir, { CI: "true" });
    assert(result.status === 1);
    assertCliSnapshot(result);
    assert(!await exists(pending));
  } finally {
    await rm(path.join(dir, ".bastest"), { recursive: true, force: true });
    await rm(path.join(dir, "src", "snapshots"), {
      recursive: true,
      force: true,
    });
  }
});

test("CLI reads snapshots with CRLF frontmatter", async () => {
  const dir = path.join(cliFixtures, "snapshot", "pending");
  const source = path.join(dir, "src", "snapshot_test.ts");
  const snapshotsDir = path.join(dir, "src", "snapshots");
  try {
    await writeFile(source, crlfSnapshotSource, "utf8");
    await rm(path.join(dir, ".bastest"), { recursive: true, force: true });
    await rm(snapshotsDir, { recursive: true, force: true });
    await mkdir(snapshotsDir, { recursive: true });
    await writeFile(
      path.join(snapshotsDir, "snapshot_test__records_value.snap"),
      [
        "---",
        "source: src/snapshot_test.ts",
        'expression: { name: "bastest", ok: true }',
        "---",
        "{",
        '  "name": "bastest",',
        '  "ok": true',
        "}",
        "",
      ].join("\r\n"),
      "utf8",
    );

    const result = runCliIn(dir);
    assert(result.status === 0, result.stdout + result.stderr);
    assertCliSnapshot(result);
  } finally {
    await writeFile(source, snapshotSource, "utf8");
    await rm(path.join(dir, ".bastest"), { recursive: true, force: true });
    await rm(snapshotsDir, {
      recursive: true,
      force: true,
    });
  }
});
