# Third-party tools

The application invokes these unmodified executables as separate processes.

- yt-dlp: https://github.com/yt-dlp/yt-dlp — version in `licenses/yt-dlp-version.txt`, license in `licenses/yt-dlp-LICENSE.txt`. Official Windows builds include third-party dependencies; their notices and source information are provided by the upstream release.
- FFmpeg / FFprobe: https://ffmpeg.org/ — Windows essentials build from https://www.gyan.dev/ffmpeg/builds/ . Build license and configuration/source references are retained in `licenses/FFmpeg-LICENSE.txt` and `licenses/FFmpeg-README.txt`. This GPL-enabled build includes libx264. Exact binary checksums are in `licenses/tool-checksums.json`.

On macOS, `scripts/macos-tools.json` pins the official universal yt-dlp release and the architecture-specific static FFmpeg / FFprobe builds by https://ffmpeg.martin-riedl.de/ . Archive SHA-256 values are checked before extraction. The bundle retains GPLv3 text, upstream build configuration and dependency versions, source/build-script links, yt-dlp's third-party license aggregation, and final tool checksums under `Contents/Resources/licenses`. Tool binaries are not linked into the application. The enclosing macOS beta uses ad-hoc signing and is not notarized.

Rust and JavaScript dependency versions are recorded in Cargo.lock and package-lock.json. They remain subject to their respective upstream licenses. No yt-dlp or FFmpeg code is incorporated into this project's MIT source files.
