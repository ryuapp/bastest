import { assert, test } from "bastest";

function accepts(value: string | undefined) {
  // @ts-expect-error bastest fixture intentionally fails typechecking.
  assert<string>(value);
}

test("should not run", () => assert(accepts("1") === undefined));
