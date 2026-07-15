import { assertType, test } from "bastest";

test("supports typed assertions", () => {
  const un = undefined;

  assertType<{ a: number }>({ a: 1 });
  assertType<string>("bastest");
  assertType<string | number>(1);
  assertType<undefined>(un);
});
