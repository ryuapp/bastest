import { assert, test } from "bastest";

declare global {
  namespace JSX {
    interface IntrinsicElements {
      [name: string]: Record<string, unknown>;
    }
  }
}

const React = {
  createElement(type: unknown, props: Record<string, unknown> | null) {
    return { type, props };
  },
};

function View() {
  return <span data-kind="example">bastest</span>;
}

test("supports tsx test files", () => {
  const element = <View />;
  assert(element.type === View);
});
