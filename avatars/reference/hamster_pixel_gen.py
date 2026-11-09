#!/usr/bin/env python3
"""Writes the pixel Hamster pack (rust/crates/aipet-sprite/packs/hamster/) from the user's reference image.

The reference (hamster.png) is pixel art at about 5.1 screen px per cell. HAMSTER below is its 28 × 28 cells, read by
sampling each cell's middle and matching it to the palette, then tidied by hand. The pack splits it into layers: the
body (the reference without its eyes, arms and feet), the arms resting on the belly and the feet (what the body
lacks), and the eyes, each a channel's values. What the reference doesn't show (raised, waving, typing and running
arms, the laptop, the magnifier, the headphones and the effects) is drawn here in its palette and its outline.

The art is 3 logical px per cell, in a 43 × 40 cell box (the app's 130 × 120 art box), with the feet on the anchor.
Every frame is the whole box, so a layer's frames sit side by side on its sheet.

Run it from anywhere: python3 avatars/reference/hamster_pixel_gen.py [--pack <folder>]. The Rust tests check the pack loads and covers
every state (pet_states.STATES).
"""

import json
import os
import struct
import sys
import zlib

import pet_states as ps

PIXEL = 3
BOX_W, BOX_H = 43, 40
# where the reference's top-left cell goes in the box
OX, OY = 7, 9
ANCHOR = (21, 37)

PALETTE = {
    "O": "#D89163",  # outline, salmon
    "L": "#F8EAB6",  # highlight
    "Y": "#F5CD75",  # head
    "R": "#F4B236",  # orange
    "C": "#F2EBD9",  # cream
    "S": "#E0D3B8",  # cream in shade
    "G": "#A5967A",  # the cream's outline
    "E": "#6B3F20",  # eyes
    "e": "#C08A5C",  # eyes' lids, lighter
    "K": "#26262C",  # headphones and laptop, dark
    "k": "#3A3A43",  # their cups and lid
    "W": "#FFFFFF",
    "g": ps.GLASS,
    "r": ps.RING,
    "h": ps.HANDLE,
    "s": ps.SPARK,
    "z": ps.ZZ,
    "b": ps.BANG,
    "d": ps.DUSTC,
    "n": "#F4B236",  # notes take the accent
    "t": "#7FEFFF",  # the screen's glow
    # fnt-72: the darker orange of the right side and bottom right, the blush, the yawning mouth, the news envelope's
    # paper and its heart seal, tears, the poke's heart and the done's golden glow (its soft rings see-through)
    "D": "#E39A2E", "B": "#FF9AAE", "M": "#8E4A3A", "P": ps.PAPER, "p": ps.PAPER_SHADE, "T": ps.TEAR,
    "H": ps.HEART, "A": ps.AURA, "a": ps.AURA_HI, "1": ps.AURA + "88", "2": ps.AURA + "44",
}

# The colours a theme may change, by name: each an exact colour in the sheets of every layer but the effects (the
# headphones' and laptop's darks, the white and the props' and effects' colours stay).
NAMED = {"outline": "O", "hi": "L", "head": "Y", "orange": "R", "cream": "C", "creamShade": "S", "creamEdge": "G",
         "eye": "E", "lid": "e", "glow": "t"}

# The reference, cell for cell (" " is see-through).
HAMSTER = [
    "   OOOO        OOOOO        ",
    "  OLLLLO      OLLLLLO       ",
    " OLYRRRYO    OLYRRRYLO      ",
    " OLYRRRYOOOOOOLYRRRYLO      ",
    " OLYOOOYOLLLLOLYOOOYLO      ",
    " OLYOOOYYYYYYYYYOOOYLO      ",
    "  OOYYYYYYYYYYYYYYYLOLOO    ",
    "   OYEEYYYYYYYEEYYYYYYLLO   ",
    "  OLYEEYOOOYYYEEYYYYYYYYLO  ",
    " OLYYYYYCOCCYYYYYYYYYYYYRLO ",
    "OLYYYYYCOCOCCCCCCCCYYYYRRRLO",
    "ORRRYYCCCCCCCCCCCCCCCCRRRRRO",
    "ORRROCCCCCCCCCCCCCCCCCRRRRRO",
    "OOOOCCCCCCCCCCCCCCCCCSROOORO",
    "OOOCCCCCCCCCCCCCCCCCCGSOOORO",
    "OSSGSCCCSSSSSSSGCCCCCGSSOORO",
    "OSSGSCCCSGSSSSSGCCCCSGSSSOOO",
    "OSSGSCCCSGSSSSSSGCSSGSSSSSOO",
    " GSSGCCSGSSSSSSSSOOGSSSSSSOO",
    "  GSSOOGSSSSSSSSSOOSSSSCCCCO",
    "  GSSOOSSSSSSSSSSSSSSSCCCCCG",
    "   GSSSSSSSSSSSSSSSSSSCCCCCG",
    "    GSSSSSGSSSSSSSSSGCCCCCG ",
    "     GSSSSSGGGGGGGGGCCCCCCG ",
    "      GSSSSSSG  ORRRGCCCCG  ",
    "       GGGGGG    OOO GGGG   ",
    "          OO           OO   ",
    "          OO           OO   ",
]

# The body: the reference with its eyes, the arms on the belly and the feet taken out (the cells under them filled).
BODY_FIX = {
    # eyes, back to the head's yellow
    (7, 5): "Y", (7, 6): "Y", (8, 5): "Y", (8, 6): "Y",
    (7, 14): "Y", (7, 15): "Y", (8, 14): "Y", (8, 15): "Y",
}
# the arms resting on the belly: their outline and paws (row, column), filled with the belly's shade without them
LEFT_ARM = [(15, 3), (16, 3), (17, 3), (18, 4), (19, 5), (19, 6), (20, 5), (20, 6), (19, 7), (18, 8), (17, 9), (16, 9)]
RIGHT_ARM = [(15, 15), (16, 15), (17, 16), (18, 17), (18, 18), (19, 17), (19, 18), (18, 19), (17, 20), (16, 21),
             (15, 21), (14, 21)]
ARM_CELLS = LEFT_ARM + RIGHT_ARM
FEET_ROWS = (26, 27)


def grid(rows):
    return [list(r) for r in rows]


def blank():
    return [[" "] * BOX_W for _ in range(BOX_H)]


def stamp(box, art, x, y):
    """`art` (rows of cells) onto `box` with its top-left at cell (x, y); spaces leave the box as it is."""
    for j, row in enumerate(art):
        for i, c in enumerate(row):
            if c != " " and 0 <= y + j < BOX_H and 0 <= x + i < BOX_W:
                box[y + j][x + i] = c
    return box


def hamster_cells():
    return grid(HAMSTER)


def body(yawn=False, blush=False):
    """The body: light from the upper left, so the right side and the bottom right are a darker orange, its edge the
    darkest (fnt-72); `yawn` opens the mouth (tired), `blush` pinks the cheeks (done)."""
    h = hamster_cells()
    for (r, c), v in BODY_FIX.items():
        h[r][c] = v
    for r, c in ARM_CELLS:
        h[r][c] = "S" if h[r][c] in "GO" else h[r][c]
    for r in FEET_ROWS:
        h[r] = [" "] * len(h[r])
    for r in range(6, 26):
        for c in range(len(h[r])):
            if c >= 22 and h[r][c] in "YCS":
                h[r][c] = "R"
            elif c >= 25 and h[r][c] == "R":
                h[r][c] = "D"
            elif r >= 22 and c >= 18 and h[r][c] in "CS":
                h[r][c] = "R"
    if yawn:
        for r, c in [(9, 9), (9, 10), (10, 9), (10, 10)]:
            h[r][c] = "M"
    if blush:
        for r, c in BLUSH_CELLS:
            h[r][c] = "B"
    return stamp(blank(), h, OX, OY)


# the cheeks that blush (row, column of the reference): the near one wide, the far one small
BLUSH_CELLS = [(9, 21), (9, 22), (9, 23), (9, 3)]


def blush():
    """The blush alone, for the poke's heart (the done's is the body's)."""
    h = [[" "] * 28 for _ in range(28)]
    for r, c in BLUSH_CELLS:
        h[r][c] = "B"
    return placed(h)


def glow():
    """The done's golden glow behind the pet (the owner's soft glow, fnt-72 task 7): two soft rings round its
    outline, the inner one stronger."""
    shape = body()
    near = lambda x, y, d: any(0 <= y + j < BOX_H and 0 <= x + i < BOX_W and shape[y + j][x + i] != " "
                               for j in range(-d, d + 1) for i in range(-d, d + 1))
    out = blank()
    for y in range(BOX_H):
        for x in range(BOX_W):
            if shape[y][x] == " ":
                out[y][x] = "1" if near(x, y, 1) else ("2" if near(x, y, 3) else " ")
    return out


def diff(cells):
    """The reference's cells at `cells`, alone."""
    h = hamster_cells()
    out = [[" "] * len(h[0]) for _ in h]
    for r, c in cells:
        out[r][c] = h[r][c]
    return out


def feet_rest():
    return diff([(r, c) for r in FEET_ROWS for c in range(28) if HAMSTER[r][c] != " "])


# ---- arms: cream arms in the salmon outline, with salmon paws, as (art, column, row) in reference cells ----
UP = ["OOO", "OCO", "OCO", "OSO", "OSO"]
LEAN = ["OO   ", "OCO  ", " OCO ", "  OSO", "   OS"]
SIDE = ["OCO", "OSO", "OO "]
SIDE_BACK = ["OCO", "OSO", " OO"]


def sprites(*parts):
    """Reference-grid art from pieces placed at (column, row), which may stick out of the reference's grid."""
    return [(art, x, y) for art, x, y in parts]


ARMS = {
    # at rest the paws are small nubs on the belly (fnt-72)
    "rest": sprites((["OO", "OO"], 7, 17), (["OO", "OO"], 17, 17)),
    "raised": sprites((UP, -3, 4), (UP, 27, 4)),
    "wave-a": sprites((diff(LEFT_ARM), 0, 0), (UP, 28, 2)),
    "wave-b": sprites((diff(LEFT_ARM), 0, 0), (LEAN, 25, 4)),
    "run-a": sprites((SIDE, -2, 11), (SIDE_BACK, 27, 13)),
    "run-b": sprites((SIDE_BACK, -2, 13), (SIDE, 27, 11)),
    "hold": sprites((["OO", "OO"], 8, 16), (["OO", "OO"], 16, 16)),
    # reading through its glasses, the paws stay nubs on the belly (fnt-72)
    "lens": sprites((["OO", "OO"], 7, 17), (["OO", "OO"], 17, 17)),
}


def arms(kind):
    box = blank()
    for art, x, y in ARMS[kind]:
        stamp(box, art, OX + x, OY + y)
    return box


def feet(kind):
    f = feet_rest()
    # two rows more, for feet that dangle
    out = [[" "] * 28 for _ in range(30)]
    shift = {"stand": (0, 0), "tap": (0, -1), "dangle-a": (1, 1), "dangle-b": (0, 1),
             "run-a": (-1, -1), "run-b": (1, 0)}
    for r in FEET_ROWS:
        for c in range(28):
            if f[r][c] == " ":
                continue
            left = c < 14
            dx, dy = shift[kind]
            # the tap and the run move one foot; dangling feet swing in step
            if kind == "tap" and left:
                dx, dy = 0, 0
            if kind.startswith("run") and not left:
                dx, dy = -dx, (-1 if dy == 0 else 0)
            if kind.startswith("dangle") and not left:
                dx = -dx
            out[r + dy][c + dx] = f[r][c]
    return out


# ---- eyes: 2 × 2 cells each, at rows 7–8, columns 5 and 14 of the reference ----
EYE_AT = [(5, 7), (14, 7)]
EYES = {
    "open": ["EE", "EE"],
    "wide": ["WE", "EE", "EE"],
    "up": ["EE", "Ee", "  "],
    "down": ["  ", "eE", "EE"],
    "blink": ["  ", "EE"],
    "squint": ["  ", "EE"],
    "closed": ["  ", "  ", "EE"],
    "happy": [" EE ", "E  E"],
    # tired (fnt-72): the lower half under a heavy lid
    "sleepy": ["ee", "EE"],
}


def eyes(kind):
    a = [[" "] * 28 for _ in range(28)]
    art = EYES[kind]
    for x, y in EYE_AT:
        w = len(art[0])
        x0 = x - (w - 2) // 2
        y0 = y - (1 if kind == "wide" else 0) - (1 if kind == "up" else 0)
        y0 = y0 - (1 if kind in ("closed",) else 0)
        for j, row in enumerate(art):
            for i, c in enumerate(row):
                if c != " ":
                    a[y0 + j][x0 + i] = c
    return a


# ---- the laptop, the magnifier and the headphones ----
def laptop():
    lid = ["kkkkkkkkkkkkkkk"] * 6
    rows = [list(r) for r in lid]
    rows[0] = list("KKKKKKKKKKKKKKK")
    for r in rows:
        r[0] = r[-1] = "K"
    rows[3][7] = "t"
    base = list("KKKKKKKKKKKKKKKKK")
    a = [[" "] * 28 for _ in range(28)]
    for j, r in enumerate(rows):
        for i, c in enumerate(r):
            a[16 + j][5 + i] = c
    for i, c in enumerate(base):
        a[22][4 + i] = c
    return a


def screen():
    a = [[" "] * 28 for _ in range(28)]
    for i in range(13):
        a[15][6 + i] = "t"
    return a


def lens():
    """The Hamster's prop for web search and reading (fnt-72): round dark glasses over its eyes, a bridge between
    them and a temple back to the near ear, instead of the magnifier."""
    # rims wider than the eyes, so the face shows round them (narrower rims read as sunglasses)
    ring = [" KKKK ", "K    K", "K    K", "K    K", " KKKK "]
    a = [[" "] * 28 for _ in range(28)]
    for x in (3, 12):
        for j, row in enumerate(ring):
            for i, c in enumerate(row):
                if c != " ":
                    a[5 + j][x + i] = c
    for x in range(9, 12):
        a[6][x] = "K"
    for x, y in [(18, 6), (19, 6), (20, 5)]:
        a[y][x] = "K"
    return a


def envelope():
    """News (fnt-72): a white envelope where the laptop stands, its flap a V, sealed with a heart."""
    w, h = 15, 8
    a = [[" "] * 28 for _ in range(28)]
    for j in range(h):
        for i in range(w):
            edge = j in (0, h - 1) or i in (0, w - 1)
            a[15 + j][5 + i] = "K" if edge else "P"
    for k in range(1, 6):
        a[15 + k][5 + k] = a[15 + k][5 + w - 1 - k] = "p"
    for x, y in [(11, 20), (13, 20), (11, 21), (12, 21), (13, 21), (12, 22)]:
        a[y][x] = "H"
    return a


def headphones():
    a = [[" "] * 28 for _ in range(28)]
    for x in range(8, 14):
        a[2][x] = "K"
    a[3][7] = a[3][14] = "K"
    # the cups over where the ears meet the head
    for y in range(5, 9):
        a[y][0] = a[y][1] = "k"
        a[y][22] = a[y][23] = "k"
    for y in (5, 8):
        a[y][0] = a[y][23] = "K"
    return a


# ---- effects, in box cells (they go above and beside the head) ----
def fx_frames(kind):
    frames = []
    if kind == "zs":
        small = ["zzzz", "  z ", " z  ", "zzzz"]
        big = ["zzzzz", "   z ", "  z  ", " z   ", "zzzzz"]
        for step in range(3):
            f = blank()
            stamp(f, small, 32, 10 - step)
            stamp(f, big, 37, 4 - step)
            frames.append(f)
    elif kind == "dots":
        for n in range(1, 4):
            f = blank()
            for i in range(n):
                stamp(f, ["WW", "WW"], 30 + 3 * i, 6)
            frames.append(f)
    elif kind == "notes":
        note = [" nn", " n ", "nn ", "nn "]
        for step in range(2):
            f = blank()
            stamp(f, note, 33, 6 - step)
            stamp(f, note, 6, 8 + step)
            frames.append(f)
    elif kind == "alert":
        bang = ["bb", "bb", "bb", "  ", "bb"]
        for step in range(2):
            f = blank()
            stamp(f, bang, 33, 4 - step)
            frames.append(f)
    elif kind == "sparkles":
        star = [" s ", "sWs", " s "]
        dot = ["s"]
        for step in range(3):
            f = blank()
            spots = [(3, 8), (36, 12), (5, 24)]
            for k, (x, y) in enumerate(spots):
                stamp(f, star if (k + step) % 3 else dot, x, y)
            frames.append(f)
    elif kind == "dust":
        for step in range(2):
            f = blank()
            stamp(f, [" dd", "ddd"], 8 - 2 * step, 34)
            stamp(f, ["dd"], 4 - 2 * step, 35 - step)
            # speed lines behind it, beside the dust (fnt-72)
            for y, w in [(16, 4), (20, 6), (24, 3)]:
                stamp(f, ["d" * w], 1 - step + (6 - w), y)
            frames.append(f)
    elif kind == "tears":
        # from under the screen-right eye: one on the cheek, one falling, both there from the first frame
        for step in range(3):
            f = blank()
            stamp(f, ["T", "T"], OX + 15, OY + 9)
            stamp(f, ["T"], OX + 16, OY + 11 + step)
            frames.append(f)
    elif kind == "heart":
        big = ["HH HH", "HHHHH", " HHH ", "  H  "]
        small = ["H H", "HHH", " H "]
        for step in range(2):
            f = blank()
            stamp(f, big, 34, 6 - step)
            stamp(f, small, 2, 12 - step)
            frames.append(f)
    elif kind == "aura":
        star = [" a ", "aAa", " a "]
        for step in range(2):
            f = blank()
            for k, (x, y) in enumerate([(5, 5), (33, 4), (20, 1)]):
                stamp(f, star if (k + step) % 2 else ["A"], x + (1 if (k + step) % 2 == 0 else 0),
                      y + (1 if (k + step) % 2 == 0 else 0))
            frames.append(f)
    return frames


def placed(art):
    """Reference-grid art in the box."""
    return stamp(blank(), art, OX, OY)


# ---- the pack ----
def colour(c):
    """A cell's RGBA: `#RRGGBB`, or `#RRGGBBAA` for a see-through one (the glow's soft rings)."""
    hexc = PALETTE[c][1:]
    v = int(hexc[:6], 16)
    return (v >> 16 & 255, v >> 8 & 255, v & 255, int(hexc[6:], 16) if len(hexc) == 8 else 255)


def sheet(frames):
    """Frames side by side: RGBA rows."""
    w, h = BOX_W * len(frames), BOX_H
    rows = []
    for y in range(h):
        row = []
        for f in frames:
            row.extend((0, 0, 0, 0) if c == " " else colour(c) for c in f[y])
        rows.append(row)
    return w, h, rows


def png(w, h, rows):
    raw = b"".join(b"\x00" + bytes(v for p in r for v in p) for r in rows)

    def chunk(t, c):
        return struct.pack(">I", len(c)) + t + c + struct.pack(">I", zlib.crc32(t + c) & 0xFFFFFFFF)

    return (b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 6, 0, 0, 0))
            + chunk(b"IDAT", zlib.compress(raw, 9)) + chunk(b"IEND", b""))


FX = ["zs", "dots", "notes", "alert"]
# fnt-72's effects replace sparkles rather than come with them
NEW_FX = ["tears", "aura", "heart"]
FX_VALUES = ["none", "dust", "sparkles"] + FX + [f"{v}+sparkles" for v in FX + ["dust"]] + NEW_FX


def pack():
    layers, sheets = [], {}

    def add(name, channel, default, values, ms=1000, clock=None, **rest):
        """A layer: values are {value: [frames]}; one sheet with every value's frames in turn."""
        frames, anims, at = [], {}, 0
        for value, fs in values.items():
            durations = ms[value] if isinstance(ms, dict) else ms
            anims[value] = {"sheet": f"{name}.png",
                            "frames": [[BOX_W * (at + i), 0, BOX_W, BOX_H, durations] for i in range(len(fs))],
                            "loop": "loop"}
            if clock:
                anims[value]["clock"] = clock
            frames.extend(fs)
            at += len(fs)
        sheets[f"{name}.png"] = png(*sheet(frames))
        layer = {"name": name, "channel": channel}
        layer.update(rest)
        layer["default"] = default
        layer["animations"] = anims
        layers.append(layer)

    # the done's golden glow first, behind everything (fnt-72)
    add("aura-glow", "fx", "aura", {"aura": [glow()]}, fallback={v: None for v in FX_VALUES if v != "aura"})
    # fnt-72: the yawn when tired, the blush when done
    add("body", "state", "idle", {"idle": [body()], "tired": [body(yawn=True)], "done": [body(blush=True)]},
        hit=True)
    add("feet", "feet", "stand", {k: [placed(feet(k))] for k in
                                  ["stand", "tap", "dangle-a", "dangle-b", "run-a", "run-b"]}, hit=True)
    add("prop", "prop", "laptop", {"laptop": [placed(laptop())], "lens": [placed(lens())],
                                   "envelope": [placed(envelope())]},
        fallback={"none": None})
    add("screen", "prop", "laptop", {"laptop": [placed(screen())]}, glow=True,
        fallback={"lens": None, "none": None, "envelope": None})
    add("arms", "arms", "rest", {k: [arms(k)] for k in ARMS}, hit=True)
    add("headphones", "headphones", "on", {"on": [placed(headphones())]}, hit=True, fallback={"off": None})
    add("eyes", "eyes", "open", {k: [placed(eyes(k))] for k in EYES}, hit=True,
        look={"center": [0, 0], "left": [-PIXEL, 0], "right": [PIXEL, 0], "up": [0, -PIXEL], "down": [0, PIXEL],
              "up-left": [-PIXEL, -PIXEL], "up-right": [PIXEL, -PIXEL], "down-left": [-PIXEL, PIXEL],
              "down-right": [PIXEL, PIXEL],
              # the eyes can't go further on the head: far is as near
              "far-left": [-PIXEL, 0], "far-right": [PIXEL, 0], "up-far-left": [-PIXEL, -PIXEL],
              "up-far-right": [PIXEL, -PIXEL], "down-far-left": [-PIXEL, PIXEL], "down-far-right": [PIXEL, PIXEL]})
    durations = {"zs": 420, "dots": 320, "notes": 360, "alert": 260}
    add("fx", "fx", "zs", {k: fx_frames(k) for k in FX}, ms=durations, clock="pet",
        fallback={**{f"{v}+sparkles": v for v in FX},
                  **{v: None for v in ["none", "dust", "dust+sparkles", "sparkles"] + NEW_FX}})
    add("sparkles", "fx", "sparkles", {"sparkles": fx_frames("sparkles")}, ms=140, clock="pet",
        fallback={v: None for v in ["none", "dust"] + FX + NEW_FX})
    # fnt-72: tears, the poke's heart with a blush, and the glow's stars; each replaces sparkles
    others = lambda value: {v: None for v in FX_VALUES if v != value}
    add("tears", "fx", "tears", {"tears": fx_frames("tears")}, ms=300, clock="pet", fallback=others("tears"))
    add("blush", "fx", "heart", {"heart": [blush()]}, fallback=others("heart"))
    add("heart", "fx", "heart", {"heart": fx_frames("heart")}, ms=220, clock="pet", fallback=others("heart"))
    add("aura", "fx", "aura", {"aura": fx_frames("aura")}, ms=200, clock="pet", fallback=others("aura"))
    add("dust", "fx", "dust", {"dust": fx_frames("dust")}, ms=110, clock="pet", mirror="flip",
        fallback={"dust+sparkles": "dust", **{v: None for v in FX_VALUES if v not in ("dust", "dust+sparkles")}})
    manifest = {
        "format_version": 1,
        "name": "Hamster (pixel)",
        # a style of the Hamster (FNT-74): one entry in Settings, its drawings as styles
        "pet": "Hamster",
        "kind": "pixel",
        "pixel_size": PIXEL,
        "anchor": list(ANCHOR),
        # drawn in perspective, turned to the left: the whole pet mirrors when it faces right (only perspective pets
        # mirror whole; effects are drawn as for a pet facing right)
        "mirror": "flip",
        "faces": "left",
        "accent": "$orange",
        "glow": "$glow",
        "palette": {name: PALETTE[c] for name, c in NAMED.items()},
        "layers": layers,
    }
    return manifest, sheets


def main():
    here = os.path.dirname(os.path.abspath(__file__))
    # --pack [folder], as the vector generators take it
    args = sys.argv[1:]
    given = args[args.index("--pack") + 1:] if "--pack" in args else []
    out = given[0] if given else os.path.join(here, "..", "..", "rust", "crates", "aipet-sprite", "packs",
                                              "hamster-pixel")
    os.makedirs(out, exist_ok=True)
    manifest, sheets = pack()
    with open(os.path.join(out, "pack.json"), "w") as f:
        json.dump(manifest, f, indent=1)
        f.write("\n")
    for name, data in sheets.items():
        with open(os.path.join(out, name), "wb") as f:
            f.write(data)


if __name__ == "__main__":
    main()
