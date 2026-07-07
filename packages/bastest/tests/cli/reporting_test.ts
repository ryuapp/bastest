import { readFile } from "node:fs/promises";
import path from "node:path";
import { test } from "bastest";
import {
  assert,
  assertCliSnapshot,
  cliFixtures,
  fixtureArg,
  fixtures,
  packageRoot,
  runCli,
  runCliIn,
  runCliWithEnv,
} from "./helpers.ts";

test("CLI uses minimal output when agent config is enabled", () => {
  const result = runCliIn(path.join(cliFixtures, "reporting", "agent-config"));
  assert(result.status === 1);
  assertCliSnapshot(result);
});

test("CLI wraps long power assert values", () => {
  const result = runCli(fixtureArg("cli", "reporting", "long-values_test.ts"));
  assert(result.status === 1);
  assertCliSnapshot(result);
});

test("CLI keeps closing quote on exact-width wrapped object values", () => {
  const result = runCliWithEnv(
    packageRoot,
    { COLUMNS: "60" },
    fixtureArg("cli", "reporting", "exact-width-object_test.ts"),
  );
  assert(result.status === 1);
  assertCliSnapshot(result);
});

test("CLI truncates very long power assert values", () => {
  const result = runCliWithEnv(
    packageRoot,
    { COLUMNS: "60" },
    fixtureArg("cli", "reporting", "very-long-value_test.ts"),
  );
  assert(result.status === 1);
  assertCliSnapshot(result);
});

test("CLI enables minimal output from agent env", async () => {
  const dir = path.join(cliFixtures, "reporting", "agent-env");
  const result = runCliWithEnv(dir, { AI_AGENT: "1" });
  assert(result.status === 1);
  assertCliSnapshot(result);
  const report = JSON.parse(
    await readFile(
      path.join(dir, ".bastest", "reports", "latest", "result.json"),
      "utf8",
    ),
  );
  assert(report.summary.failed === 1);
  assert(report.failures.length === 1);
});

test("CLI enables minimal output from --agent", () => {
  const result = runCliIn(
    path.join(cliFixtures, "reporting", "agent-flag"),
    "--agent",
  );
  assert(result.status === 1);
  assertCliSnapshot(result);
});

test("CLI does not print report path for successful agent runs", () => {
  const result = runCliIn(
    path.join(cliFixtures, "reporting", "agent-success"),
    "--agent",
  );
  assert(result.status === 0);
  assertCliSnapshot(result);
});

test("CLI report output matches pass skip fail fixture", async () => {
  const file = path.join(fixtures, "report", "basic_test.ts");

  const result = runCli(file);
  assert(result.status === 1);
  assertCliSnapshot(result);
});
