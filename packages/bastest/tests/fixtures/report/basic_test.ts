import { assert, test } from "bastest";

test("fixture pass", () => {
  assert(1 + 1 === 2);
});

test.ignore("fixture skip", () => {
  assert(false);
});

test("fixture fail", () => {
  const user = { name: "alpha" };
  const expected = "beta";
  assert(user.name === expected);
});
