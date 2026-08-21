#!/usr/bin/env python3
"""Build the fixed 720x360 forest-map demo asset.

The runtime asset is deliberately simple: one unsigned byte per map cell.
255 means water/no estimate, 0 means land where forest is not shown, and 100
means the JRC visualization marks estimated forest presence. The WMS source is
visualization-only, so this tool intentionally does not infer percentages.

This is an offline developer tool. It requires Pillow, but the application and
its Docker image do not require Python or Pillow at runtime.
"""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
from typing import Iterable

from PIL import Image, ImageDraw, __version__ as PILLOW_VERSION


OUTPUT_WIDTH = 720
OUTPUT_HEIGHT = 360
EXPECTED_FOREST_RGBA = (77, 146, 33, 255)
NO_ESTIMATE = 255
FOREST_PRESENT = 100
EXPECTED_FOREST_SHA256 = "04c0faee578a370537cb28f6a99957bdd2de880fdc09d7d6eb069905f2dbc7c6"
EXPECTED_LAND_SHA256 = "9e0729ee253ca7d7a5c4ae9395fb1902264c5377c52e224d13dd85010e2835d9"
EXPECTED_OUTPUT_SHA256 = "970e006dac8927e4aa7e659eab20d295f244aab92567f58caf629ce013a7a944"
EXPECTED_PILLOW_VERSION = "12.3.0"


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def pixel_point(longitude: float, latitude: float) -> tuple[int, int]:
    x = round((longitude + 180.0) / 360.0 * (OUTPUT_WIDTH - 1))
    y = round((90.0 - latitude) / 180.0 * (OUTPUT_HEIGHT - 1))
    return (
        min(OUTPUT_WIDTH - 1, max(0, x)),
        min(OUTPUT_HEIGHT - 1, max(0, y)),
    )


def polygon_parts(geometry: dict) -> Iterable[list[list[list[float]]]]:
    geometry_type = geometry.get("type")
    coordinates = geometry.get("coordinates")
    if geometry_type == "Polygon":
        yield coordinates
    elif geometry_type == "MultiPolygon":
        yield from coordinates
    else:
        raise ValueError(f"unsupported Natural Earth geometry: {geometry_type!r}")


def build_land_mask(land_geojson: Path) -> Image.Image:
    document = json.loads(land_geojson.read_text(encoding="utf-8"))
    if document.get("type") != "FeatureCollection":
        raise ValueError("Natural Earth input must be a GeoJSON FeatureCollection")

    mask = Image.new("1", (OUTPUT_WIDTH, OUTPUT_HEIGHT), 0)
    draw = ImageDraw.Draw(mask)
    for feature in document.get("features", []):
        for rings in polygon_parts(feature["geometry"]):
            if not rings:
                continue
            draw.polygon([pixel_point(*point) for point in rings[0]], fill=1)
            for hole in rings[1:]:
                draw.polygon([pixel_point(*point) for point in hole], fill=0)
    return mask


def build_forest_presence(forest_png: Path) -> list[int]:
    source = Image.open(forest_png).convert("RGBA")
    expected_size = (OUTPUT_WIDTH, OUTPUT_HEIGHT)
    if source.size != expected_size:
        raise ValueError(
            f"JRC snapshot must be {expected_size[0]}x{expected_size[1]}, got "
            f"{source.size[0]}x{source.size[1]}"
        )

    values: list[int] = []
    for rgba in source.get_flattened_data():
        if rgba == EXPECTED_FOREST_RGBA:
            values.append(FOREST_PRESENT)
        elif rgba == (0, 0, 0, 0):
            values.append(0)
        else:
            raise ValueError(f"unexpected JRC WMS pixel value: {rgba!r}")
    return values


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--forest-png", required=True, type=Path)
    parser.add_argument("--land-geojson", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()

    if PILLOW_VERSION != EXPECTED_PILLOW_VERSION:
        raise ValueError(
            f"Pillow {EXPECTED_PILLOW_VERSION} is required for a deterministic asset; "
            f"found {PILLOW_VERSION}"
        )

    forest_hash = sha256(args.forest_png)
    land_hash = sha256(args.land_geojson)
    if forest_hash != EXPECTED_FOREST_SHA256:
        raise ValueError(
            "JRC snapshot hash changed; review the new source before updating the pinned hash"
        )
    if land_hash != EXPECTED_LAND_SHA256:
        raise ValueError(
            "Natural Earth geometry hash changed; review it before updating the pinned hash"
        )

    land_mask = build_land_mask(args.land_geojson)
    land = list(land_mask.get_flattened_data())
    forest = build_forest_presence(args.forest_png)
    output = bytearray(OUTPUT_WIDTH * OUTPUT_HEIGHT)
    for index, forest_value in enumerate(forest):
        # The forest source is more detailed than the 1:110m coastline. Keep a
        # positive coastal/island forest sample even if Natural Earth omits it.
        output[index] = forest_value if land[index] or forest_value else NO_ESTIMATE

    output_hash = hashlib.sha256(output).hexdigest()
    if output_hash != EXPECTED_OUTPUT_SHA256:
        raise ValueError(
            "derived map hash changed; review the rasterizer output before updating the pin"
        )

    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_bytes(output)
    print(f"wrote {len(output)} bytes to {args.output}")
    print(f"forest snapshot sha256: {forest_hash}")
    print(f"land GeoJSON sha256:     {land_hash}")
    print(f"derived asset sha256:    {output_hash}")


if __name__ == "__main__":
    main()
