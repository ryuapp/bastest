import { test } from "bastest";
import { assert, assertCliSnapshot, fixtureArg, runCli } from "./helpers.ts";

test("CLI runs explicit test files", () => {
  const result = runCli(fixtureArg("cli", "run", "pass", "sample_test.ts"));
  assert(result.status === 0, result.stderr);
  assertCliSnapshot(result);
});

test("CLI loads optimized dependencies in CommonJS packages", () => {
  const result = runCli(
    fixtureArg("cli", "run", "commonjs", "sample_test.js"),
  );
  assert(result.status === 0, result.stderr);
});

test("CLI isolates each test file in its own process", () => {
  const result = runCli(
    "--concurrency=1",
    fixtureArg("cli", "run", "isolation", "a_check_test.ts"),
    fixtureArg("cli", "run", "isolation", "z_mutate_test.ts"),
  );
  assert(result.status === 0, result.stdout + result.stderr);
});

test("CLI returns non-zero when a test fails", () => {
  const result = runCli(fixtureArg("cli", "run", "fail", "sample_test.ts"));
  assert(result.status === 1);
  assertCliSnapshot(result);
});
