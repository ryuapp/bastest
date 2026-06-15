import { assert, test } from "bastest";

test("supports typed assertions", () => {
  assert<{ a: number }>({ a: 1 });
  assert<string>("bastest");
  assert<string | number>(1);
});
