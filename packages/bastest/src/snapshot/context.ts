export interface SnapshotContext {
  cwd?: string;
  file: string;
  testName: string;
  testNameOccurrence: number;
  index: number;
}

export interface AssertSnapshotOptions {
  name?: string;
  expression?: string;
}

export interface AssertSnapshotMetadata {
  expression?: string;
}
