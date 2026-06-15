const version = Deno.args[0]?.replace(/^v/, "");

if (!version) {
  console.error(
    "usage: deno run --allow-read --allow-write scripts/prepare-npm-package.ts <version>",
  );
  Deno.exit(2);
}

const root = Deno.cwd();
const sourcePackage = pathJoin(root, "packages", "bastest");
const outPackage = pathJoin(root, "dist", "bastest");

const optionalDependencies = {
  "@bastest/darwin-arm64": version,
  "@bastest/linux-arm64-musl": version,
  "@bastest/linux-x64-musl": version,
  "@bastest/win32-arm64": version,
  "@bastest/win32-x64": version,
};

await Deno.remove(outPackage, { recursive: true }).catch(() => {});
await Deno.mkdir(outPackage, { recursive: true });
await copyDir(pathJoin(sourcePackage, "bin"), pathJoin(outPackage, "bin"));
await copyDir(pathJoin(sourcePackage, "dist"), pathJoin(outPackage, "dist"));

const packageJson = JSON.parse(
  await Deno.readTextFile(pathJoin(sourcePackage, "package.json")),
);
packageJson.version = version;
packageJson.optionalDependencies = optionalDependencies;
delete packageJson.devDependencies;

await Deno.writeTextFile(
  pathJoin(outPackage, "package.json"),
  `${JSON.stringify(packageJson, null, 2)}\n`,
);

for await (const entry of walkFiles(outPackage)) {
  if (basename(entry).endsWith(".map")) {
    await Deno.remove(entry);
  }
}

async function copyDir(from: string, to: string): Promise<void> {
  await Deno.mkdir(to, { recursive: true });
  for await (const entry of Deno.readDir(from)) {
    const source = pathJoin(from, entry.name);
    const target = pathJoin(to, entry.name);
    if (entry.isDirectory) {
      await copyDir(source, target);
    } else if (entry.isFile) {
      await Deno.copyFile(source, target);
    }
  }
}

async function* walkFiles(dir: string): AsyncGenerator<string> {
  for await (const entry of Deno.readDir(dir)) {
    const path = pathJoin(dir, entry.name);
    if (entry.isDirectory) {
      yield* walkFiles(path);
    } else if (entry.isFile) {
      yield path;
    }
  }
}

function basename(path: string): string {
  return path.split(/[\\/]/).at(-1) ?? path;
}

function pathJoin(...parts: string[]): string {
  return parts.join(Deno.build.os === "windows" ? "\\" : "/");
}
