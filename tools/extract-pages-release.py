#!/usr/bin/env python3
"""Safely extract one prebuilt public-catalog release for GitHub Pages."""

from __future__ import annotations

import argparse
import os
from pathlib import Path, PurePosixPath
import shutil
import stat
import tarfile
import tempfile
from typing import NoReturn


MAX_ARCHIVE_BYTES = 512 * 1024 * 1024
MAX_ENTRIES = 128
MAX_EXTRACTED_BYTES = 1024 * 1024 * 1024
COPY_BUFFER_BYTES = 1024 * 1024


def fail(message: str) -> NoReturn:
    raise SystemExit(f"pages release extraction failed: {message}")


def normalized_member_path(name: str, is_directory: bool) -> PurePosixPath | None:
    if not name or "\0" in name or "\\" in name or name.startswith("/"):
        fail(f"archive member has an unsafe path: {name!r}")

    while name.startswith("./"):
        name = name[2:]
    name = name.rstrip("/")
    if name in {"", "."}:
        if is_directory:
            return None
        fail("the archive root entry must be a directory")

    raw_parts = name.split("/")
    path = PurePosixPath(name)
    if path.is_absolute() or any(part in {"", ".", ".."} for part in raw_parts):
        fail(f"archive member has an unsafe path: {name!r}")
    if any(":" in part for part in path.parts):
        fail(f"archive member has a platform-ambiguous path: {name!r}")
    return path


def inspect_archive(
    archive: tarfile.TarFile,
) -> list[tuple[tarfile.TarInfo, PurePosixPath]]:
    members: list[tarfile.TarInfo] = []
    for member in archive:
        members.append(member)
        if len(members) > MAX_ENTRIES:
            fail(f"archive has more than {MAX_ENTRIES} entries")
    if not members:
        fail("archive is empty")

    inspected: list[tuple[tarfile.TarInfo, PurePosixPath]] = []
    kinds: dict[PurePosixPath, str] = {}
    extracted_bytes = 0
    for member in members:
        if not (member.isfile() or member.isdir()):
            fail(f"archive contains a link or special entry: {member.name!r}")
        path = normalized_member_path(member.name, member.isdir())
        if path is None:
            continue
        if path in kinds:
            fail(f"archive contains a duplicate entry: {path.as_posix()!r}")

        for parent in path.parents:
            if parent == PurePosixPath("."):
                break
            if kinds.get(parent) == "file":
                fail(f"archive places an entry below a file: {path.as_posix()!r}")

        kind = "directory" if member.isdir() else "file"
        kinds[path] = kind
        if member.isfile():
            if member.size < 0:
                fail(f"archive file has a negative size: {path.as_posix()!r}")
            extracted_bytes += member.size
            if extracted_bytes > MAX_EXTRACTED_BYTES:
                fail(
                    f"archive expands beyond {MAX_EXTRACTED_BYTES} bytes"
                )
        inspected.append((member, path))

    file_paths = {path for _, path in inspected if kinds[path] == "file"}
    for file_path in file_paths:
        prefix = file_path.parts
        if any(
            other != file_path and other.parts[: len(prefix)] == prefix
            for other in kinds
        ):
            fail(f"archive uses a file as a directory: {file_path.as_posix()!r}")

    for required in (PurePosixPath("index.html"), PurePosixPath("catalog-manifest.json")):
        if required not in file_paths:
            fail(f"archive is not rooted at the release directory (missing {required})")

    return inspected


def copy_member(
    archive: tarfile.TarFile,
    member: tarfile.TarInfo,
    destination: Path,
) -> None:
    source = archive.extractfile(member)
    if source is None:
        fail(f"could not read archive member: {member.name!r}")

    destination.parent.mkdir(mode=0o755, parents=True, exist_ok=True)
    flags = os.O_WRONLY | os.O_CREAT | os.O_EXCL
    descriptor = os.open(destination, flags, 0o644)
    remaining = member.size
    try:
        with os.fdopen(descriptor, "wb") as output:
            descriptor = -1
            while remaining:
                chunk = source.read(min(COPY_BUFFER_BYTES, remaining))
                if not chunk:
                    fail(f"archive member ended early: {member.name!r}")
                output.write(chunk)
                remaining -= len(chunk)
            if source.read(1):
                fail(f"archive member exceeds its declared size: {member.name!r}")
    finally:
        source.close()
        if descriptor >= 0:
            os.close(descriptor)


def extract_release(archive_path: Path, output_path: Path) -> None:
    try:
        archive_metadata = archive_path.lstat()
    except FileNotFoundError:
        fail(f"archive does not exist: {archive_path}")
    if not stat.S_ISREG(archive_metadata.st_mode):
        fail(f"archive must be a regular non-symlink file: {archive_path}")
    if archive_metadata.st_size <= 0 or archive_metadata.st_size > MAX_ARCHIVE_BYTES:
        fail(f"archive size is outside the allowed range: {archive_metadata.st_size} bytes")

    if os.path.lexists(output_path):
        fail(f"output path already exists: {output_path}")
    output_parent = output_path.parent
    if not output_parent.is_dir() or output_parent.is_symlink():
        fail(f"output parent must be a real directory: {output_parent}")

    temporary = Path(
        tempfile.mkdtemp(prefix=f".{output_path.name}.extract-", dir=output_parent)
    )
    promoted = False
    try:
        try:
            with tarfile.open(archive_path, mode="r:gz") as archive:
                inspected = inspect_archive(archive)
                for _, path in sorted(
                    (item for item in inspected if item[0].isdir()),
                    key=lambda item: len(item[1].parts),
                ):
                    (temporary / Path(*path.parts)).mkdir(
                        mode=0o755, parents=True, exist_ok=False
                    )
                for member, path in (item for item in inspected if item[0].isfile()):
                    copy_member(archive, member, temporary / Path(*path.parts))
        except (OSError, tarfile.TarError) as error:
            fail(f"could not read archive: {error}")

        temporary.replace(output_path)
        promoted = True
    finally:
        if not promoted:
            shutil.rmtree(temporary, ignore_errors=True)


def main() -> None:
    parser = argparse.ArgumentParser(
        description="Safely extract a prebuilt Waajacu public release archive."
    )
    parser.add_argument("archive", type=Path)
    parser.add_argument("output", type=Path)
    arguments = parser.parse_args()
    extract_release(arguments.archive.absolute(), arguments.output.absolute())
    print(f"Safely extracted release archive to {arguments.output}")


if __name__ == "__main__":
    main()
