import { assert, test } from "bastest";

test("generated fail", () => {
  const user = { name: "alpha" };
  const expected = "beta";
  assert(user.name === expected);
});
