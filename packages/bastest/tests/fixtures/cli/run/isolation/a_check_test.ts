import { assert, test } from "bastest";

test("does not inherit process-global state", () => {
  assert(!("__bastest_isolation_probe" in globalThis));
});
