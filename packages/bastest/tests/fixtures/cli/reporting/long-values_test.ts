import { assert, test } from "bastest";

test("long string value", () => {
  const actual = "x".repeat(140);
  const expected = "y";
  assert(actual === expected);
});

test("long expected value", () => {
  const actual = "short";
  const expected = "y".repeat(140);
  assert(actual === expected);
});

test("long object value", () => {
  const actual = { message: "z".repeat(140) };
  const expected = { message: "ok" };
  assert(actual.message === expected.message);
});

test("long unicode value", () => {
  const actual = "長".repeat(140);
  const expected = "短";
  assert(actual === expected);
});
