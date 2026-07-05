import { assert, test } from "bastest";

test("very long value", () => {
  const actual = { message: "a".repeat(500) };
  const expected = { message: "ok" };
  assert(actual.message === expected.message);
});
