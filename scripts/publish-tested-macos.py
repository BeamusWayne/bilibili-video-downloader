"""Copy successful, checksum-verified native artifacts into an existing draft."""
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys


def api(path):
    return json.loads(subprocess.check_output(["gh", "api", path], text=True))


def metadata():
    repository = os.environ["GITHUB_REPOSITORY"]
    run_id = os.environ["BVD_RUN_ID"]
    tag = os.environ["BVD_RELEASE_TAG"]
    if not run_id.isascii() or not run_id.isdigit() or not re.fullmatch(r"v\d+\.\d+\.\d+-beta\.\d+", tag):
        raise ValueError("Expected a numeric run id and beta version tag")
    run = api(f"repos/{repository}/actions/runs/{run_id}")
    if (run["conclusion"] != "success" or run["name"] != "macOS DMG"
            or run["head_repository"]["full_name"] != repository):
        raise ValueError("Source must be a successful native macOS run in this repository")
    # The REST by-tag endpoint omits drafts; gh also checks the draft list.
    release = json.loads(subprocess.check_output([
        "gh", "release", "view", tag, "--repo", repository,
        "--json", "isDraft,isPrerelease,targetCommitish",
    ], text=True))
    if not release["isDraft"] or not release["isPrerelease"] or release["targetCommitish"] != run["head_sha"]:
        raise ValueError("Release must be a draft prerelease targeting the tested commit")
    return repository, tag, run["head_sha"]


def upload():
    repository, tag, source = metadata()
    root = Path("tested-macos")
    assets = []
    for arch, binary_arch in (("arm64", "arm64"), ("x64", "x86_64")):
        name = f"BilibiliVideoDownloader_{tag[1:]}_macos-{arch}.dmg"
        dmg = root / name
        report = root / f"{name}.verification.json"
        checksum = root / f"{name}.sha256"
        info = json.loads(report.read_text(encoding="utf-8"))
        digest = hashlib.file_digest(dmg.open("rb"), "sha256").hexdigest()
        if (info["sourceCommit"] != source or info["architecture"] != binary_arch
                or info["sha256"] != digest or checksum.read_text().strip() != f"{digest}  {name}"):
            raise ValueError(f"Artifact identity or checksum failed: {name}")
        assets.extend([str(dmg), str(report), str(checksum)])
    subprocess.run(["gh", "release", "upload", tag, *assets, "--repo", repository], check=True)
    print("Uploaded both verified DMGs and reports; release remains a draft.")


if __name__ == "__main__":
    if sys.argv[1:] == ["metadata"]:
        metadata()
        print("Source run and draft release target verified.")
    elif sys.argv[1:] == ["upload"]:
        upload()
    else:
        raise SystemExit("Expected metadata or upload")
