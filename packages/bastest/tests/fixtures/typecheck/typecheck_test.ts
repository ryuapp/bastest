import { assert, assertType, test } from "bastest";

function accepts(value: string | undefined) {
  // @ts-expect-error bastest fixture intentionally asserts a narrower type.
  assertType<string>(value);
}

test("should not run after typecheck failure", () => {
  assert(accepts("ok") === undefined);
});
