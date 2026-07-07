import { assert, test } from "bastest";

if (import.meta.test) {
  test("should not be discovered", () => assert(false));
}
