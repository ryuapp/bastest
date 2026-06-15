# Bastest

Test runner for JavaScript runtimes.

## Features

- In-source tests with `import.meta.test`
- Minimal agent-friendly reporting
- Type testing support

## Example

```ts
import { assert, test } from "bastest";

test("adds two numbers", () => {
  assert(1 + 1 === 2);
});
```
