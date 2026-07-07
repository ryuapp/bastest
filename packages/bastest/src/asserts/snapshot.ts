import {
  type AssertSnapshotMetadata,
  type AssertSnapshotOptions,
} from "../snapshot/context.ts";
import { getTestExecutionContext } from "../runtime.ts";
import { assertSnapshotWithContext } from "../snapshot/core.ts";

export type { AssertSnapshotMetadata, AssertSnapshotOptions };

export function assertSnapshot(
  value: unknown,
  options?: string | AssertSnapshotOptions,
  metadata?: AssertSnapshotMetadata,
): void {
  const context = getTestExecutionContext();
  if (!context) {
    throw new Error("assertSnapshot must be called inside a bastest test");
  }

  assertSnapshotWithContext(
    context.snapshot,
    context.runtime,
    value,
    options,
    metadata,
  );
}
