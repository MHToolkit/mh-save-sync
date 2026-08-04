#!/usr/bin/env python3
"""Create the self-authored raster artwork for the native MH3G converter.

The artwork intentionally uses simple, deterministic geometry rather than game
screenshots or downloaded visual material.  Keeping the generator in-tree
makes the release assets reproducible and license-safe.
"""

from __future__ import annotations

import argparse
import math
import random
import struct
import zlib
from pathlib import Path


WIDTH = 1024
HEIGHT = 576


class Canvas:
    def __init__(self, width: int, height: int, seed: int) -> None:
        self.width = width
        self.height = height
        self.pixels = bytearray(width * height * 3)
        rng = random.Random(seed)
        for y in range(height):
            for x in range(width):
                depth = y / max(height - 1, 1)
                horizon = 0.5 + 0.5 * math.sin((x / width) * math.pi)
                grain = rng.randrange(-3, 4)
                r = int(18 + 10 * (1 - depth) + 3 * horizon + grain)
                g = int(40 + 33 * (1 - depth) + 7 * horizon + grain)
                b = int(55 + 44 * (1 - depth) + 14 * horizon + grain)
                self.set(x, y, (r, g, b))

    def set(self, x: int, y: int, color: tuple[int, int, int]) -> None:
        if 0 <= x < self.width and 0 <= y < self.height:
            offset = (y * self.width + x) * 3
            self.pixels[offset : offset + 3] = bytes(color)

    def blend(self, x: int, y: int, color: tuple[int, int, int], alpha: float) -> None:
        if 0 <= x < self.width and 0 <= y < self.height:
            offset = (y * self.width + x) * 3
            for channel in range(3):
                self.pixels[offset + channel] = int(
                    self.pixels[offset + channel] * (1 - alpha) + color[channel] * alpha
                )

    def circle(self, cx: float, cy: float, radius: float, color: tuple[int, int, int], alpha: float = 1.0) -> None:
        min_x = max(0, int(cx - radius - 1))
        max_x = min(self.width - 1, int(cx + radius + 1))
        min_y = max(0, int(cy - radius - 1))
        max_y = min(self.height - 1, int(cy + radius + 1))
        radius_sq = radius * radius
        for y in range(min_y, max_y + 1):
            for x in range(min_x, max_x + 1):
                dx = x - cx
                dy = y - cy
                distance_sq = dx * dx + dy * dy
                if distance_sq <= radius_sq:
                    feather = min(1.0, (radius_sq - distance_sq) / max(radius * 1.8, 1))
                    self.blend(x, y, color, alpha * feather)

    def rect(self, left: int, top: int, right: int, bottom: int, color: tuple[int, int, int], alpha: float = 1.0) -> None:
        for y in range(max(0, top), min(self.height, bottom)):
            for x in range(max(0, left), min(self.width, right)):
                self.blend(x, y, color, alpha)

    def rounded_rect(self, left: int, top: int, right: int, bottom: int, radius: int, color: tuple[int, int, int], alpha: float = 1.0) -> None:
        for y in range(max(0, top), min(self.height, bottom)):
            for x in range(max(0, left), min(self.width, right)):
                corner_x = min(x - left, right - 1 - x)
                corner_y = min(y - top, bottom - 1 - y)
                if corner_x >= radius or corner_y >= radius:
                    self.blend(x, y, color, alpha)
                else:
                    distance = (corner_x - radius) ** 2 + (corner_y - radius) ** 2
                    if distance <= radius * radius:
                        self.blend(x, y, color, alpha)

    def line(self, x0: float, y0: float, x1: float, y1: float, width: float, color: tuple[int, int, int], alpha: float = 1.0) -> None:
        distance = max(1, int(math.hypot(x1 - x0, y1 - y0)))
        for step in range(distance + 1):
            t = step / distance
            self.circle(x0 + (x1 - x0) * t, y0 + (y1 - y0) * t, width / 2, color, alpha)

    def polygon(self, points: list[tuple[float, float]], color: tuple[int, int, int], alpha: float = 1.0) -> None:
        min_x = max(0, int(min(x for x, _ in points)))
        max_x = min(self.width - 1, int(max(x for x, _ in points)))
        min_y = max(0, int(min(y for _, y in points)))
        max_y = min(self.height - 1, int(max(y for _, y in points)))
        for y in range(min_y, max_y + 1):
            for x in range(min_x, max_x + 1):
                inside = False
                j = len(points) - 1
                for i, (xi, yi) in enumerate(points):
                    xj, yj = points[j]
                    if (yi > y) != (yj > y) and x < (xj - xi) * (y - yi) / (yj - yi) + xi:
                        inside = not inside
                    j = i
                if inside:
                    self.blend(x, y, color, alpha)

    def horizon(self, y: int) -> None:
        self.rect(0, y, self.width, self.height, (10, 29, 37), 0.30)
        for index in range(11):
            x = index * 120 - 80
            peak = y - 35 - (index % 3) * 22
            self.polygon([(x, y), (x + 88, peak), (x + 160, y)], (18, 50, 60), 0.35)

    def glow(self, cx: int, cy: int, radius: int, color: tuple[int, int, int]) -> None:
        for ring in range(radius, 0, -4):
            self.circle(cx, cy, ring, color, 0.008 + (radius - ring) / (radius * 220))

    def png(self) -> bytes:
        raw = bytearray()
        stride = self.width * 3
        for y in range(self.height):
            raw.append(0)
            raw.extend(self.pixels[y * stride : (y + 1) * stride])
        def chunk(kind: bytes, data: bytes) -> bytes:
            return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data) & 0xFFFFFFFF)
        return b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", self.width, self.height, 8, 2, 0, 0, 0)) + chunk(b"IDAT", zlib.compress(bytes(raw), 9)) + chunk(b"IEND", b"")


def draw_route() -> Canvas:
    canvas = Canvas(WIDTH, HEIGHT, 10)
    canvas.horizon(376)
    canvas.glow(528, 275, 170, (235, 174, 73))
    canvas.rounded_rect(125, 194, 344, 391, 28, (14, 29, 38), 0.96)
    canvas.rounded_rect(145, 217, 324, 347, 17, (43, 88, 100), 0.9)
    canvas.rounded_rect(174, 369, 294, 384, 7, (89, 133, 142), 0.70)
    canvas.rounded_rect(685, 153, 905, 344, 24, (12, 30, 39), 0.97)
    canvas.rounded_rect(707, 177, 883, 305, 13, (47, 90, 101), 0.92)
    canvas.rect(746, 344, 844, 368, (31, 65, 74), 0.95)
    canvas.line(348, 302, 682, 244, 14, (232, 165, 67), 0.82)
    canvas.line(491, 277, 551, 265, 28, (255, 200, 99), 0.25)
    canvas.polygon([(646, 230), (687, 244), (654, 270)], (255, 204, 104), 0.93)
    return canvas


def draw_components() -> Canvas:
    canvas = Canvas(WIDTH, HEIGHT, 20)
    canvas.horizon(402)
    for left, top, color in [(168, 210, (38, 89, 92)), (265, 210, (43, 102, 101)), (168, 303, (46, 110, 107)), (265, 303, (39, 92, 96))]:
        canvas.rounded_rect(left, top, left + 75, top + 62, 12, color, 0.95)
        canvas.rounded_rect(left + 9, top + 9, left + 66, top + 47, 7, (83, 134, 132), 0.52)
    canvas.rounded_rect(120, 162, 377, 397, 32, (18, 46, 54), 0.48)
    for left, top in [(610, 198), (709, 198), (610, 302), (709, 302)]:
        canvas.circle(left + 38, top + 36, 39, (68, 104, 106), 0.85)
        canvas.circle(left + 38, top + 36, 28, (156, 126, 76), 0.38)
        canvas.line(left + 14, top + 22, left + 62, top + 49, 5, (225, 170, 79), 0.55)
    canvas.rounded_rect(560, 152, 819, 405, 32, (20, 49, 57), 0.46)
    canvas.line(421, 286, 546, 286, 5, (231, 169, 73), 0.66)
    canvas.circle(485, 286, 22, (238, 176, 80), 0.42)
    return canvas


def draw_flow() -> Canvas:
    canvas = Canvas(WIDTH, HEIGHT, 30)
    canvas.horizon(385)
    canvas.rounded_rect(114, 214, 242, 351, 22, (25, 69, 78), 0.90)
    canvas.rounded_rect(782, 205, 911, 342, 22, (25, 69, 78), 0.90)
    canvas.polygon([(468, 165), (585, 229), (558, 393), (441, 331)], (63, 133, 140), 0.55)
    canvas.polygon([(490, 196), (557, 233), (540, 337), (474, 301)], (112, 177, 176), 0.35)
    canvas.line(247, 284, 438, 274, 10, (228, 164, 72), 0.78)
    canvas.line(584, 274, 775, 274, 10, (228, 164, 72), 0.78)
    canvas.circle(318, 280, 29, (243, 189, 96), 0.42)
    canvas.circle(685, 280, 29, (243, 189, 96), 0.42)
    canvas.rounded_rect(442, 419, 575, 474, 18, (42, 86, 96), 0.92)
    canvas.circle(508, 446, 14, (232, 171, 76), 0.85)
    canvas.rounded_rect(305, 434, 404, 484, 16, (46, 91, 99), 0.78)
    return canvas


def draw_rollback() -> Canvas:
    canvas = Canvas(WIDTH, HEIGHT, 40)
    canvas.horizon(387)
    canvas.glow(524, 290, 180, (218, 164, 77))
    canvas.rounded_rect(427, 232, 610, 374, 24, (23, 61, 68), 0.96)
    canvas.rounded_rect(448, 263, 588, 354, 16, (69, 128, 131), 0.50)
    canvas.circle(519, 306, 28, (230, 174, 77), 0.90)
    canvas.line(716, 290, 611, 306, 12, (228, 167, 75), 0.70)
    canvas.line(420, 306, 292, 333, 12, (228, 167, 75), 0.70)
    canvas.polygon([(293, 333), (330, 311), (325, 352)], (246, 191, 95), 0.90)
    canvas.rounded_rect(737, 227, 839, 343, 20, (40, 87, 96), 0.80)
    canvas.circle(788, 285, 25, (176, 202, 199), 0.48)
    return canvas


def draw_cec() -> Canvas:
    canvas = Canvas(WIDTH, HEIGHT, 50)
    canvas.horizon(390)
    canvas.rounded_rect(435, 206, 622, 374, 24, (21, 63, 71), 0.96)
    canvas.rounded_rect(460, 239, 597, 279, 10, (92, 148, 149), 0.68)
    canvas.rounded_rect(506, 285, 553, 372, 8, (46, 104, 111), 0.85)
    canvas.circle(331, 218, 14, (193, 218, 210), 0.55)
    canvas.circle(278, 271, 10, (193, 218, 210), 0.45)
    canvas.circle(684, 232, 12, (239, 180, 87), 0.55)
    canvas.circle(741, 293, 9, (239, 180, 87), 0.42)
    canvas.line(344, 230, 442, 264, 4, (163, 210, 200), 0.42)
    canvas.line(608, 270, 730, 295, 4, (230, 173, 79), 0.48)
    canvas.rounded_rect(710, 164, 848, 387, 27, (112, 165, 164), 0.12)
    canvas.rounded_rect(730, 187, 828, 365, 19, (218, 237, 227), 0.12)
    return canvas


def icon_canvas(size: int) -> Canvas:
    canvas = Canvas(size, size, 77)
    scale = size / 1024
    def s(value: int) -> int:
        return max(1, round(value * scale))
    canvas.circle(s(512), s(512), s(410), (18, 75, 83), 0.95)
    canvas.rounded_rect(s(168), s(270), s(430), s(679), s(45), (13, 38, 48), 0.98)
    canvas.rounded_rect(s(198), s(308), s(400), s(525), s(29), (59, 138, 144), 0.92)
    canvas.rounded_rect(s(594), s(290), s(857), s(639), s(45), (13, 38, 48), 0.98)
    canvas.rounded_rect(s(624), s(326), s(827), s(512), s(29), (59, 138, 144), 0.92)
    canvas.line(s(432), s(488), s(593), s(466), s(36), (239, 174, 76), 0.90)
    canvas.polygon([(s(547), s(421)), (s(613), s(466)), (s(552), s(506))], (255, 205, 105), 0.95)
    return canvas


def write_png(path: Path, canvas: Canvas) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(canvas.png())


def write_ico(path: Path, images: list[tuple[int, bytes]]) -> None:
    offset = 6 + 16 * len(images)
    header = struct.pack("<HHH", 0, 1, len(images))
    records = bytearray()
    payload = bytearray()
    for size, data in images:
        records.extend(struct.pack("<BBBBHHII", 0 if size == 256 else size, 0 if size == 256 else size, 0, 0, 1, 32, len(data), offset))
        payload.extend(data)
        offset += len(data)
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(header + records + payload)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--repo-root", type=Path, default=Path(__file__).resolve().parents[1])
    args = parser.parse_args()
    root = args.repo_root.resolve()
    artwork = root / "apps/mh3g-save-converter-macos/Resources/Artwork"
    for name, draw in {
        "input-route.png": draw_route,
        "components-workshop.png": draw_components,
        "dry-run-flow.png": draw_flow,
        "rollback-harbor.png": draw_rollback,
        "cec-mailbox.png": draw_cec,
    }.items():
        write_png(artwork / name, draw())

    iconset = root / "artifacts/mh3g-save-converter-icon.iconset"
    iconset.mkdir(parents=True, exist_ok=True)
    icon_specs = [(16, "icon_16x16.png"), (32, "icon_16x16@2x.png"), (32, "icon_32x32.png"), (64, "icon_32x32@2x.png"), (128, "icon_128x128.png"), (256, "icon_128x128@2x.png"), (256, "icon_256x256.png"), (512, "icon_256x256@2x.png"), (512, "icon_512x512.png"), (1024, "icon_512x512@2x.png")]
    icon_pngs: dict[int, bytes] = {}
    for size, name in icon_specs:
        image = icon_canvas(size).png()
        (iconset / name).write_bytes(image)
        icon_pngs[size] = image
    windows_icon = root / "apps/mh3g-save-converter-windows/Assets/MH3GSaveConverter.ico"
    write_ico(windows_icon, [(16, icon_pngs[16]), (32, icon_pngs[32]), (48, icon_canvas(48).png()), (256, icon_pngs[256])])
    print(f"Generated artwork in {artwork}")
    print(f"Generated iconset in {iconset}")
    print(f"Generated Windows icon at {windows_icon}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
