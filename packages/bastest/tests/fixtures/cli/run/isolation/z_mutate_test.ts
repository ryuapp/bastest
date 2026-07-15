import { assert, test } from "bastest";

test("mutates process-global state", () => {
  Object.assign(globalThis, { __bastest_isolation_probe: "leaked" });
  assert(true);
});
