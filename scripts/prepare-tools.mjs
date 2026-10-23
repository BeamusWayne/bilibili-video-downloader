import { execFileSync } from "node:child_process";
import { fileURLToPath } from "node:url";

if (process.platform === "win32") {
  execFileSync(
    "powershell",
    [
      "-NoProfile",
      "-ExecutionPolicy",
      "Bypass",
      "-File",
      fileURLToPath(new URL("prepare-tools.ps1", import.meta.url)),
    ],
    { stdio: "inherit", windowsHide: true },
  );
} else if (process.platform === "darwin") {
  await import("./prepare-macos-tools.mjs");
} else {
  throw new Error("Packaged tools currently support Windows and macOS only.");
}
