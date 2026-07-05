import path from "node:path";
import { test } from "bastest";
import {
  assert,
  assertCliSnapshot,
  cliFixtures,
  fixtures,
  runCliIn,
} from "./helpers.ts";

test("CLI runs typecheck before tests when enabled", () => {
  const result = runCliIn(path.join(cliFixtures, "typecheck", "enabled"));
  assert(result.status !== 0);
  assertCliSnapshot(result);
});

test("CLI typecheck output matches fixture", async () => {
  const cwd = path.join(fixtures, "typecheck");

  const result = runCliIn(cwd);
  assert(result.status === 1);
  assertCliSnapshot(result);
});
