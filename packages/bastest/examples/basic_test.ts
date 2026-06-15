import { assert, test } from "bastest";

test("adds numbers", () => {
  assert(1 + 1 === 2);
});

test("supports steps", async (t) => {
  await t.step("prepare", () => {
    assert(true);
  });

  await t.step({
    name: "nested",
    fn: async (step) => {
      await step.step("inner", () => {
        assert("bastest".includes("test"));
      });
    },
  });
});

test.ignore("ignored test", () => {
  throw new Error("should not run");
});
