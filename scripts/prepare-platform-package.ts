const [platform, binaryPath, rawVersion] = Deno.args;
const version = rawVersion?.replace(/^v/, "");

if (!platform || !binaryPath || !version) {
  console.error(
    "usage: deno run --allow-read --allow-write scripts/prepare-platform-package.ts <platform> <binary-path> <version>",
  );
  Deno.exit(2);
}

const metadata = platformMetadata(platform);
const packageDir = pathJoin(Deno.cwd(), "dist", "@bastest", platform);
const binaryName = basename(binaryPath).endsWith(".exe")
  ? "bastest.exe"
  : "bastest";

await Deno.remove(packageDir, { recursive: true }).catch(() => {});
await Deno.mkdir(packageDir, { recursive: true });
await Deno.copyFile(binaryPath, pathJoin(packageDir, binaryName));

if (!binaryName.endsWith(".exe")) {
  await Deno.chmod(pathJoin(packageDir, binaryName), 0o755);
}

const packageJson = {
  name: `@bastest/${platform}`,
  version,
  description: `${metadata.label} binary for bastest.`,
  license: "MIT",
  os: [metadata.os],
  cpu: [metadata.cpu],
  ...(metadata.libc ? { libc: [metadata.libc] } : {}),
  files: [binaryName],
  publishConfig: {
    access: "public",
  },
};

await Deno.writeTextFile(
  pathJoin(packageDir, "package.json"),
  `${JSON.stringify(packageJson, null, 2)}\n`,
);

function platformMetadata(
  platform: string,
): { label: string; os: string; cpu: string; libc?: string } {
  switch (platform) {
    case "linux-x64-musl":
      return { label: "Linux x64 musl", os: "linux", cpu: "x64", libc: "musl" };
    case "linux-arm64-musl":
      return {
        label: "Linux arm64 musl",
        os: "linux",
        cpu: "arm64",
        libc: "musl",
      };
    case "darwin-arm64":
      return { label: "macOS arm64", os: "darwin", cpu: "arm64" };
    case "win32-x64":
      return { label: "Windows x64", os: "win32", cpu: "x64" };
    case "win32-arm64":
      return { label: "Windows arm64", os: "win32", cpu: "arm64" };
    default:
      console.error(`unknown platform: ${platform}`);
      Deno.exit(2);
  }
}

function basename(path: string): string {
  return path.split(/[\\/]/).at(-1) ?? path;
}

function pathJoin(...parts: string[]): string {
  return parts.join(Deno.build.os === "windows" ? "\\" : "/");
}
