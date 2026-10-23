import { execFileSync, spawn } from "node:child_process";
import { createHash } from "node:crypto";
import {
  access,
  mkdtemp,
  readFile,
  readdir,
  realpath,
  rm,
  writeFile,
} from "node:fs/promises";
import { tmpdir, release } from "node:os";
import path from "node:path";

if (process.platform !== "darwin")
  throw new Error("Run bundle verification on macOS.");
const [dmgArgument, expectedArch] = process.argv.slice(2);
if (!dmgArgument || !["arm64", "x86_64"].includes(expectedArch)) {
  throw new Error(
    "Usage: node scripts/check-macos-bundle.mjs <dmg> <arm64|x86_64>",
  );
}
const dmg = path.resolve(dmgArgument);
// macOS /var aliases /private/var. Tauri intentionally rejects an executable
// launched through symlinks, so exercise installation using the real path.
const temporary = await realpath(
  await mkdtemp(path.join(tmpdir(), "bvd-install-check-")),
);
const mount = path.join(temporary, "mount");
const installed = path.join(
  temporary,
  "Applications Test",
  "BilibiliVideoDownloader.app",
);
let mounted = false;
let child;
let startup = "";
const run = (file, args) =>
  execFileSync(file, args, { encoding: "utf8", timeout: 120_000 });
const checks = {
  sourceCommit: process.env.GITHUB_SHA,
  architecture: expectedArch,
  macOS: release(),
  checks: [],
};
try {
  run("hdiutil", ["verify", dmg]);
  run("hdiutil", [
    "attach",
    "-readonly",
    "-nobrowse",
    "-mountpoint",
    mount,
    dmg,
  ]);
  mounted = true;
  await access(path.join(mount, "Applications"));
  run("ditto", [path.join(mount, "BilibiliVideoDownloader.app"), installed]);
  run("hdiutil", ["detach", mount]);
  mounted = false;
  checks.checks.push(
    "DMG verified, mounted and app copied; DMG detached before launch",
  );
  run("codesign", ["--verify", "--deep", "--strict", installed]);
  checks.checks.push(
    "Ad-hoc application code signature verified (not notarization)",
  );

  const resources = path.join(installed, "Contents/Resources");
  const tools = path.join(resources, "tools");
  const executable = path.join(
    installed,
    "Contents/MacOS/bilibili-video-downloader",
  );
  const sourceChecksums = JSON.parse(
    await readFile(
      path.join(resources, "licenses/tool-checksums.json"),
      "utf8",
    ),
  );
  for (const name of ["ffmpeg", "ffprobe", "yt-dlp"]) {
    const program = path.join(tools, name);
    const digest = createHash("sha256")
      .update(await readFile(program))
      .digest("hex");
    if (digest !== sourceChecksums.binaries[name])
      throw new Error(`Bundled ${name} checksum changed`);
    const version = run(program, [
      name === "yt-dlp" ? "--version" : "-version",
    ]).split("\n")[0];
    console.log(`${name}: ${version}`);
  }
  for (const program of [
    executable,
    ...["ffmpeg", "ffprobe", "yt-dlp"].map((name) => path.join(tools, name)),
  ]) {
    const architectures = run("lipo", ["-archs", program]).trim().split(/\s+/);
    if (!architectures.includes(expectedArch))
      throw new Error(`Wrong binary architecture: ${program}`);
    const loadCommands = run("otool", ["-arch", expectedArch, "-l", program]);
    const minimumVersions = loadCommands
      .split(/Load command \d+/)
      .flatMap((block) => {
        const buildVersion = /cmd LC_BUILD_VERSION\b/.test(block);
        if (!buildVersion && !/cmd LC_VERSION_MIN_MACOSX\b/.test(block))
          return [];
        const value = block.match(
          buildVersion
            ? /^\s*minos\s+(\d+)\.(\d+)/m
            : /^\s*version\s+(\d+)\.(\d+)/m,
        );
        return value ? [value] : [];
      });
    if (
      !minimumVersions.length ||
      minimumVersions.some(
        (match) =>
          Number(match[1]) > 13 ||
          (Number(match[1]) === 13 && Number(match[2]) > 0),
      )
    ) {
      throw new Error(
        `Binary exceeds the macOS 13 deployment target: ${program}`,
      );
    }
    for (const line of run("otool", ["-arch", expectedArch, "-L", program])
      .split("\n")
      .slice(1)) {
      const dependency = line.trim().split(" ")[0];
      if (
        dependency &&
        !dependency.startsWith("/System/Library/") &&
        !dependency.startsWith("/usr/lib/")
      ) {
        throw new Error(`Unbundled non-system dependency: ${dependency}`);
      }
    }
  }
  checks.checks.push(
    "Executable architecture and system-only dynamic library references verified",
  );
  const ffmpeg = path.join(tools, "ffmpeg");
  const ffprobe = path.join(tools, "ffprobe");
  const sample = path.join(temporary, "测试 source.mp4");
  run(ffmpeg, [
    "-v",
    "error",
    "-f",
    "lavfi",
    "-i",
    "testsrc2=size=320x240:rate=24",
    "-f",
    "lavfi",
    "-i",
    "sine=frequency=440",
    "-t",
    "1",
    "-c:v",
    "libx264",
    "-pix_fmt",
    "yuv420p",
    "-c:a",
    "aac",
    "-y",
    sample,
  ]);
  for (const [extension, codec] of [
    ["mp4", null],
    ["mkv", null],
    ["mp3", "libmp3lame"],
    ["m4a", "aac"],
    ["flac", "flac"],
    ["wav", "pcm_s16le"],
  ]) {
    const output = path.join(temporary, `转换 sample.${extension}`);
    const args = codec ? ["-vn", "-c:a", codec] : ["-c", "copy"];
    run(ffmpeg, ["-v", "error", "-i", sample, ...args, "-y", output]);
    const metadata = JSON.parse(
      run(ffprobe, [
        "-v",
        "error",
        "-show_streams",
        "-show_format",
        "-of",
        "json",
        output,
      ]),
    );
    if (
      !(Number(metadata.format.duration) > 0) ||
      !metadata.streams.some(
        (s) => s.codec_type === (codec ? "audio" : "video"),
      )
    ) {
      throw new Error(`Invalid packaged-tool ${extension} output`);
    }
  }
  checks.checks.push(
    "All six output formats verified using installed application's tools and Unicode paths",
  );
  await access(
    path.join(resources, "licenses/yt-dlp-THIRD_PARTY_LICENSES.txt"),
  );
  await access(path.join(resources, "licenses/FFmpeg-build-versions.txt"));

  // Check a real native launch from the copied app, with no developer server.
  child = spawn(executable, [], { stdio: ["ignore", "pipe", "pipe"] });
  child.stdout.on("data", (data) => {
    startup += data.toString();
  });
  child.stderr.on("data", (data) => {
    startup += data.toString();
  });
  const earlyExit = new Promise((_, reject) => {
    child.once("error", reject);
    child.once("exit", (code, signal) =>
      reject(
        new Error(
          `Native app exited before startup check: ${code}/${signal}\n${startup}`,
        ),
      ),
    );
  });
  await Promise.race([
    new Promise((resolve) => setTimeout(resolve, 8000)),
    earlyExit,
  ]);
  child.removeAllListeners("exit");
  const stopped = new Promise((resolve) => child.once("exit", resolve));
  child.kill("SIGTERM");
  await stopped;
  child = undefined;
  checks.checks.push(
    "Copied native application remained running for eight seconds without a dev server",
  );
  checks.dmg = path.basename(dmg);
  checks.sha256 = createHash("sha256")
    .update(await readFile(dmg))
    .digest("hex");
  await writeFile(`${dmg}.sha256`, `${checks.sha256}  ${checks.dmg}\n`);
  await writeFile(
    `${dmg}.verification.json`,
    JSON.stringify(checks, null, 2) + "\n",
  );
  console.log(JSON.stringify(checks, null, 2));
} finally {
  if (child && child.exitCode === null) child.kill("SIGKILL");
  if (mounted) run("hdiutil", ["detach", mount]);
  await rm(temporary, { recursive: true, force: true });
}
