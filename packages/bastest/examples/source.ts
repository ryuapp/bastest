import { assert, test } from "bastest";

export function add(a: number, b: number): number {
  return a + b;
}

if (import.meta.test) {
  test("in-source add", () => {
    assert(add(1, 2) === 3);
  });
}
