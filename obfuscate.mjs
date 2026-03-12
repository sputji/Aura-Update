// obfuscate.mjs — Pre-build : minifie + obfusque le JS frontend
import { execSync } from "child_process";
import { copyFileSync, existsSync } from "fs";

const src = "./frontend/js/app.js";
const bak = "./frontend/js/app.js.bak";

if (!existsSync(src)) {
  console.warn(`[obfuscate] ${src} introuvable, skip.`);
  process.exit(0);
}

// Sauvegarde de l'original
copyFileSync(src, bak);
console.log("[obfuscate] backup →", bak);

// Terser : minification agressive + mangling
execSync(
  `npx terser "${src}" -o "${src}" ` +
    "--compress drop_console=true,passes=3 " +
    "--mangle toplevel " +
    "--format comments=false",
  { stdio: "inherit" }
);

console.log("[obfuscate] done ✔");
