import { assert, test } from "bastest";

// @ts-expect-error bastest fixture intentionally compares disjoint literals.
test("generated fail", () => assert(1 === 2));
