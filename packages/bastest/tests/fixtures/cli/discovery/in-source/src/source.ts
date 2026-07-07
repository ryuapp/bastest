import { assert, test } from "bastest";

export const value = 1;

if (import.meta.test) {
  test("in-source pass", () => assert(value === 1));
}
