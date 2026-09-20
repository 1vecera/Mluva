import fs from "node:fs";
import { bundle } from "@remotion/bundler";
import {
  openBrowser,
  renderStill,
  selectComposition,
} from "@remotion/renderer";

const manifest = JSON.parse(fs.readFileSync("reference/captures.json", "utf8"));
fs.mkdirSync("out/fidelity", { recursive: true });
const serveUrl = await bundle({
  entryPoint: "src/index.ts",
  publicDir: "public",
});
const browser = await openBrowser("chrome", {
  chromiumOptions: { headless: true },
});
try {
  const render = async (inputProps, output) => {
    const composition = await selectComposition({
      serveUrl,
      id: "NativeReconstruction",
      inputProps,
      puppeteerInstance: browser,
    });
    await renderStill({
      serveUrl,
      composition,
      puppeteerInstance: browser,
      inputProps,
      frame: 0,
      imageFormat: "png",
      output,
      logLevel: "error",
    });
  };
  for (const capture of manifest.captures) {
    const surface = capture.file.replace(/\.png$/, "");
    await render({ surface }, `out/fidelity/${capture.file}`);
  }
  for (const clock of [0, 8, 12])
    await render(
      { surface: "grilling", clock },
      `out/fidelity/grilling-clock-${clock}.png`,
    );
  console.log(
    `Rendered ${manifest.captures.length} native states and three clock composites.`,
  );
} finally {
  await browser.close({ silent: true });
}
