// restore.mjs — Post-build : restaure le JS frontend original
import { copyFileSync, unlinkSync, existsSync } from "fs";

const src = "./frontend/js/app.js";
const bak = "./frontend/js/app.js.bak";

if (!existsSync(bak)) {
  console.warn("[restore] pas de backup trouvé, skip.");
  process.exit(0);
}

copyFileSync(bak, src);
unlinkSync(bak);
console.log("[restore] app.js restauré ✔");
