import { createHash } from "node:crypto";
import { chmod, mkdir, readFile, writeFile } from "node:fs/promises";
import { execFileSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import path from "node:path";

if (process.platform !== "darwin" || !["arm64", "x64"].includes(process.arch)) {
  throw new Error(
    "Run macOS tool preparation on an Apple Silicon or Intel Mac.",
  );
}
const root = fileURLToPath(new URL("../", import.meta.url));
const manifest = JSON.parse(
  await readFile(new URL("macos-tools.json", import.meta.url), "utf8"),
);
const arch = process.arch;
const cache = path.join(root, ".cache/macos-tools");
const binaries = path.join(root, "src-tauri/binaries/macos");
const licenses = path.join(root, "src-tauri/licenses/macos");
await Promise.all(
  [cache, binaries, licenses].map((p) => mkdir(p, { recursive: true })),
);
const sha256 = (data) => createHash("sha256").update(data).digest("hex");

async function request(url) {
  for (let attempt = 0; ; attempt++) {
    try {
      const response = await fetch(url, {
        signal: AbortSignal.timeout(300_000),
      });
      if (!response.ok) throw new Error(`HTTP ${response.status}: ${url}`);
      return Buffer.from(await response.arrayBuffer());
    } catch (error) {
      if (attempt >= 2) throw error;
      await new Promise((resolve) => setTimeout(resolve, 1000 * 2 ** attempt));
    }
  }
}
async function verified(url, expected, filename) {
  const destination = path.join(cache, filename);
  let bytes = await readFile(destination).catch(() => null);
  if (!bytes || sha256(bytes) !== expected) {
    console.log(`Downloading ${filename}`);
    bytes = await request(url);
    if (sha256(bytes) !== expected)
      throw new Error(`SHA-256 mismatch: ${filename}`);
    await writeFile(destination, bytes);
  }
  return { destination, bytes };
}

const ffmpeg = manifest.ffmpeg[arch];
for (const program of ["ffmpeg", "ffprobe"]) {
  const archive = await verified(
    `${ffmpeg.baseUrl}${program}.zip`,
    ffmpeg[program],
    `${arch}-${program}.zip`,
  );
  execFileSync(
    "unzip",
    ["-j", "-o", archive.destination, program, "-d", binaries],
    { stdio: "inherit" },
  );
  await chmod(path.join(binaries, program), 0o755);
}
const extractor = await verified(
  manifest.ytDlp.url,
  manifest.ytDlp.sha256,
  `yt-dlp-${manifest.ytDlp.version}`,
);
await writeFile(path.join(binaries, "yt-dlp"), extractor.bytes, {
  mode: 0o755,
});
await chmod(path.join(binaries, "yt-dlp"), 0o755);

const notices = [
  [
    "FFmpeg-LICENSE.txt",
    `https://raw.githubusercontent.com/FFmpeg/FFmpeg/n${manifest.ffmpeg.version}/COPYING.GPLv3`,
  ],
  ["FFmpeg-build-versions.txt", `${ffmpeg.baseUrl}versions.txt`],
  [
    "yt-dlp-LICENSE.txt",
    `https://raw.githubusercontent.com/yt-dlp/yt-dlp/${manifest.ytDlp.version}/LICENSE`,
  ],
  [
    "yt-dlp-THIRD_PARTY_LICENSES.txt",
    `https://raw.githubusercontent.com/yt-dlp/yt-dlp/${manifest.ytDlp.version}/THIRD_PARTY_LICENSES.txt`,
  ],
];
for (const [name, url] of notices)
  await writeFile(path.join(licenses, name), await request(url));
await writeFile(
  path.join(licenses, "yt-dlp-version.txt"),
  `${manifest.ytDlp.version}\n`,
);
await writeFile(
  path.join(licenses, "FFmpeg-README.txt"),
  [
    `FFmpeg / FFprobe ${manifest.ffmpeg.version}; static macOS ${arch} builds by Martin Riedl.`,
    `Original archives: ${ffmpeg.baseUrl}`,
    "Build scripts and dependency source links: https://git.martin-riedl.de/ffmpeg/build-script",
    `FFmpeg source: https://github.com/FFmpeg/FFmpeg/tree/n${manifest.ffmpeg.version}`,
    "Exact dependency versions and configuration: FFmpeg-build-versions.txt",
    "These GPLv3-enabled tools run as separate, unmodified executables.",
    "Upstream signatures are retained. The enclosing test app is ad-hoc signed, not notarized.",
    "",
  ].join("\n"),
);
const checksums = {};
for (const program of ["ffmpeg", "ffprobe", "yt-dlp"]) {
  checksums[program] = sha256(await readFile(path.join(binaries, program)));
  console.log(
    execFileSync(
      path.join(binaries, program),
      [program === "yt-dlp" ? "--version" : "-version"],
      { encoding: "utf8" },
    ).split("\n")[0],
  );
}
await writeFile(
  path.join(licenses, "tool-checksums.json"),
  JSON.stringify(
    { platform: "darwin", arch, binaries: checksums, sources: manifest },
    null,
    2,
  ) + "\n",
);

// Generate Apple's icon container from the existing application artwork.
const iconset = path.join(cache, "BilibiliVideoDownloader.iconset");
await mkdir(iconset, { recursive: true });
for (const size of [16, 32, 128, 256, 512]) {
  for (const scale of [1, 2]) {
    const pixels = String(size * scale);
    const name = `icon_${size}x${size}${scale === 2 ? "@2x" : ""}.png`;
    execFileSync(
      "sips",
      [
        "-z",
        pixels,
        pixels,
        path.join(root, "src-tauri/icons/icon.png"),
        "--out",
        path.join(iconset, name),
      ],
      { stdio: "pipe" },
    );
  }
}
execFileSync("iconutil", [
  "-c",
  "icns",
  iconset,
  "-o",
  path.join(root, "src-tauri/icons/icon.icns"),
]);
