import { assert, test } from "bastest";

test("exact width object value", () => {
  const actual = { message: "z".repeat(34) };
  const expected = { message: "ok" };
  assert(actual.message === expected.message);
});
