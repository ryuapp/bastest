import { assert, test } from "bastest";

if (import.meta.test) {
  test("excluded source", () => assert(false));
}
