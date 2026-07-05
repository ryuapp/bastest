import { test } from "bastest";
import { assert, assertCliSnapshot, fixtureArg, runCli } from "./helpers.ts";

test("CLI runs explicit test files", () => {
  const result = runCli(fixtureArg("cli", "run", "pass", "sample_test.ts"));
  assert(result.status === 0, result.stderr);
  assertCliSnapshot(result);
});

test("CLI returns non-zero when a test fails", () => {
  const result = runCli(fixtureArg("cli", "run", "fail", "sample_test.ts"));
  assert(result.status === 1);
  assertCliSnapshot(result);
});
