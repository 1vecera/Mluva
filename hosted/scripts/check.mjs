import { readdir, readFile } from "node:fs/promises";
import { spawnSync } from "node:child_process";
for (const directory of ["web", "server", "scripts", "tests"]) {
  for (const name of await readdir(directory))
    if (name.endsWith(".mjs") || name.endsWith(".js")) {
      const result = spawnSync(
        process.execPath,
        ["--check", `${directory}/${name}`],
        { stdio: "inherit" },
      );
      if (result.status) process.exit(result.status);
    }
}
JSON.parse(await readFile("web/config.json"));
JSON.parse(await readFile("web/manifest.webmanifest"));
process.stdout.write("JavaScript syntax and static JSON checked.\n");
