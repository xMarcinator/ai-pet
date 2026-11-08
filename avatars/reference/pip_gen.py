#!/usr/bin/env python3
"""Writes the vector Pip pack (rust/crates/aipet-sprite/packs/pip/pack.json) from pocket-sprout.html's drawing.

The drawing is the page's character SVG: its feet, sprout, body, belly, cheeks, eyes with highlights, lids and
mouths, in its own coordinates (the feet's middle at 0, 0; up is negative). It is scaled by K into the pack's art box
with the feet on the anchor. The page's procedural motion becomes pack data: the blink and the happy and sleeping
eyes are expressions, the look-at is the eyes' look table, the sprout's sway and droop are its part's rotations, the
walk is the feet's poses. What the page lacks (hands, headphones, the laptop and the lens, the effects) is drawn in
its style: round, soft-edged, in its palette.

Run it from anywhere; the Rust tests check the pack loads (builtin.rs) and that painted Pip is its bake.
"""

import json
import math
import os
import re

K = 0.52
ANCHOR = (65.0, 110.0)

PET, SHADE, BELLY, CHEEK = "#FFAA85", "#E48E69", "#FFC9AD", "#FF7FA0"
LEAF, EYE, WHITE, ZZ = "#5CC48C", "#161A26", "#FFFFFF", "#C9D0FF"
ACCENT, GLOW = "#8C98FF", "#7FEFFF"
METAL, METAL_DARK = "#3A4466", "#2A3352"


def n(v):
    """A number as the manifest writes it: at most 2 decimals, no trailing zeros."""
    s = f"{v:.2f}".rstrip("0").rstrip(".")
    return "0" if s in ("-0", "") else s


def art(x, y):
    """A point of the page's drawing in the art box."""
    return (ANCHOR[0] + K * x, ANCHOR[1] + K * y)


def pt(p):
    return [round(p[0], 2), round(p[1], 2)]


def tf(d, origin=None):
    """The page's path data (absolute M L Q C Z only) in the art box, or around `origin` (a point of the page's
    drawing) for a part."""
    out = []
    for tok in re.findall(r"[MLQCZ]|-?[\d.]+", d):
        out.append(tok)
    res, nums = [], []
    for tok in out:
        if tok.isalpha():
            res.append(tok)
        else:
            nums.append(float(tok))
            if len(nums) == 2:
                x, y = nums
                if origin is None:
                    x, y = art(x, y)
                else:
                    x, y = K * (x - origin[0]), K * (y - origin[1])
                res.append(f"{n(x)} {n(y)}")
                nums = []
    return " ".join(res)


def ellipse(cx, cy, rx, ry):
    """An ellipse as path data, in whatever coordinates its numbers are."""
    return (f"M {n(cx - rx)} {n(cy)} A {n(rx)} {n(ry)} 0 1 0 {n(cx + rx)} {n(cy)} "
            f"A {n(rx)} {n(ry)} 0 1 0 {n(cx - rx)} {n(cy)} Z")


def rect(x, y, w, h, r):
    """A rounded rectangle as path data."""
    return (f"M {n(x + r)} {n(y)} H {n(x + w - r)} A {n(r)} {n(r)} 0 0 1 {n(x + w)} {n(y + r)} "
            f"V {n(y + h - r)} A {n(r)} {n(r)} 0 0 1 {n(x + w - r)} {n(y + h)} H {n(x + r)} "
            f"A {n(r)} {n(r)} 0 0 1 {n(x)} {n(y + h - r)} V {n(y + r)} A {n(r)} {n(r)} 0 0 1 {n(x + r)} {n(y)} Z")


def fill(d, colour, opacity=1.0):
    s = {"d": d, "fill": colour}
    if opacity != 1.0:
        s["opacity"] = opacity
    return s


def stroke(d, colour, width, opacity=1.0):
    s = {"d": d, "stroke": colour, "stroke_width": round(width, 2), "linecap": "round", "linejoin": "round"}
    if opacity != 1.0:
        s["opacity"] = opacity
    return s


def k(v):
    return K * v


def layer(name, channel, default, **rest):
    out = {"name": name, "channel": channel}
    out.update(rest)
    out["default"] = default
    return out


def keys(frames):
    """Part animation keys from (ms, fields) pairs."""
    out = []
    for ms, f in frames:
        key = {kk: round(v, 2) for kk, v in f.items()}
        key["ms"] = ms
        out.append(key)
    return out


def anim(parts, frames, mode="loop"):
    """A part animation: (id, phase) pairs, or one id."""
    if isinstance(parts, str):
        parts = [(parts, 0)]
    out = {"parts": [p for p, _ in parts], "loop": mode, "keys": frames}
    if any(ph for _, ph in parts):
        out["phase"] = [ph for _, ph in parts]
    return out


def sway(amplitude, period_ms, steps, base=0.0):
    """A rotation back and forth as held keys, `steps` per period."""
    ms = round(period_ms / steps)
    return keys([(ms, {"rotate": base + amplitude * math.sin(2 * math.pi * i / steps)}) for i in range(steps)])


# the sprout's stem foot, which it sways and droops about
STEM = (0, -118)


def sprout():
    stem = stroke(tf("M 0 -118 Q 3 -136 0 -150", STEM), LEAF, k(4))
    left = fill(tf("M 0 -150 C -8 -164 -30 -167 -35 -156 C -26 -145 -8 -145 0 -150 Z", STEM), LEAF)
    right = fill(tf("M 0 -150 C 8 -169 33 -173 37 -160 C 28 -147 8 -145 0 -150 Z", STEM), LEAF)
    calm = sway(5, 3700, 16)
    return layer(
        "sprout", "state", "idle", hit=True,
        parts=[{"id": "sprout", "origin": pt(art(*STEM)), "shape": "sprout"}],
        shapes={"sprout": [stem, left, right]},
        animations={
            "idle": anim("sprout", calm),
            "listen": anim("sprout", calm),
            "thinking": anim("sprout", sway(4, 2600, 12, base=8)),
            "working": anim("sprout", sway(4, 900, 8)),
            "attention": anim("sprout", sway(6, 600, 6)),
            "done": anim("sprout", sway(8, 450, 6)),
            "idle-wiggle": anim("sprout", sway(10, 500, 6)),
            # asleep, it droops to one side
            "sleep": anim("sprout", sway(2, 5000, 10, base=28)),
        })


def feet():
    left, right = art(-30, -5), art(30, -5)
    lift = lambda p, dy, dx=0: pt((p[0] + dx, p[1] + dy))
    return layer(
        "feet", "feet", "stand", hit=True,
        parts=[{"id": "foot-left", "origin": pt(left), "shape": "foot"},
               {"id": "foot-right", "origin": pt(right), "shape": "foot"}],
        shapes={"foot": fill(ellipse(0, 0, k(17), k(8)), SHADE)},
        poses={
            "stand": {},
            "tap": {},
            # dangling from the pointer, the feet hang closer together
            "dangle-a": {"foot-left": lift(left, 3, 3), "foot-right": lift(right, 2, -3)},
            "dangle-b": {"foot-left": lift(left, 2, 3), "foot-right": lift(right, 3, -3)},
            # the page's walk: one foot up, then the other
            "run-a": {"foot-left": lift(left, -k(4)), "foot-right": lift(right, 0)},
            "run-b": {"foot-left": lift(left, 0), "foot-right": lift(right, -k(4))},
        },
        transition_ms=90,
        animations={"tap": anim("foot-right", [[0, -1.5, 160], [0, 0, 160]])})


def body():
    shapes = lambda cheeks: [
        fill(tf("M -72 -50 C -72 -100 -40 -122 0 -122 C 40 -122 72 -100 72 -50 C 72 -14 42 0 0 0 "
                "C -42 0 -72 -14 -72 -50 Z"), PET),
        fill(ellipse(*art(0, -30), k(38), k(22)), BELLY),
        fill(ellipse(*art(-47, -58), k(11), k(6.5)), CHEEK, cheeks),
        fill(ellipse(*art(47, -58), k(11), k(6.5)), CHEEK, cheeks),
    ]
    return layer("body", "state", "idle", hit=True,
                 shapes={"idle": shapes(0.5), "done": shapes(0.85), "attention": shapes(0.7),
                         "sleep": shapes(0.35)})


def mouth():
    m = lambda d: stroke(tf(d), EYE, k(3.5))
    smile = m("M -11 -53 Q 0 -42 11 -53")
    return layer(
        "mouth", "state", "idle", hit=False,
        shapes={
            "idle": smile,
            "listen": m("M -8 -52 Q 0 -46 8 -52"),
            "sleep": m("M -5 -50 Q 0 -47 5 -50"),
            "thinking": m("M -8 -50 L 8 -50"),
            "working": m("M -8 -51 Q 0 -47 8 -51"),
            "attention": fill(ellipse(*art(0, -50), k(6), k(6)), EYE),
            "done": [fill(ellipse(*art(0, -49), k(8), k(7)), EYE)],
        })


def eyes():
    hi = lambda dy=0.0, r=3.0: fill(ellipse(k(-2.5), k(-4.5 + dy), k(r), k(r)), WHITE)
    ball = lambda ry, rx=8.5, dy=0.0: fill(ellipse(0, k(dy), k(rx), k(ry)), EYE)
    lid = lambda d: stroke(tf(d, (0, 0)), EYE, k(4))
    # the page's look: up to 5 across and 4 up or down, in the page's px
    lx, ly = k(5), k(4)
    look = {
        "center": [0, 0],
        "up": [0, -ly], "down": [0, ly], "left": [-lx * 0.7, 0], "right": [lx * 0.7, 0],
        "up-left": [-lx * 0.6, -ly * 0.7], "up-right": [lx * 0.6, -ly * 0.7],
        "down-left": [-lx * 0.6, ly * 0.7], "down-right": [lx * 0.6, ly * 0.7],
        "far-left": [-lx, 0], "far-right": [lx, 0],
        "up-far-left": [-lx, -ly * 0.5], "up-far-right": [lx, -ly * 0.5],
        "down-far-left": [-lx, ly * 0.5], "down-far-right": [lx, ly * 0.5],
    }
    return layer(
        "eyes", "eyes", "open", hit=True,
        parts=[{"id": "eye-left", "origin": pt(art(-26, -78))}, {"id": "eye-right", "origin": pt(art(26, -78))}],
        shapes={
            "open": [ball(11.5), hi()],
            "wide": [ball(13, rx=9.5), hi(-0.5, 3.5)],
            "squint": [ball(6.9), hi(2, 2.2)],
            # the page's blink: the eye's height at 0.12
            "blink": ball(11.5 * 0.12),
            "up": [ball(11.5, dy=-1.5), hi(-2)],
            "down": [ball(10, dy=1.5), hi(2.5, 2.6)],
            # the page's happy and sleeping lids, one per eye
            "happy": lid("M -10 2 Q 0 -11 10 2"),
            "closed": lid("M -10 -1 Q 0 8 10 -1"),
        },
        look={kk: [round(v, 2) for v in d] for kk, d in look.items()})


def headphones():
    band = stroke(tf("M -66 -78 C -70 -138 70 -138 66 -78"), METAL, k(7))
    cup = lambda x: [fill(rect(*art(x - 9, -92), k(18), k(28), k(8)), METAL),
                     fill(rect(*art(x - 5, -88), k(10), k(20), k(5)), ACCENT)]
    return layer("headphones", "headphones", "on", hit=True,
                 shapes={"on": [band] + cup(-70) + cup(70)},
                 fallback={"off": None})


# where the hands are in each arms pose, in the page's px
HANDS = {
    "rest": ((-64, -36), (64, -36)),
    "raised": ((-76, -96), (76, -96)),
    "wave-a": ((-64, -36), (82, -98)),
    "wave-b": ((-64, -36), (90, -84)),
    "run-a": ((-70, -44), (62, -30)),
    "run-b": ((-62, -30), (70, -44)),
    "hold": ((-34, -26), (34, -26)),
    "lens": ((-64, -36), (48, -60)),
}


def arms():
    rest_l, rest_r = HANDS["rest"]
    poses = {}
    for value, (l, r) in HANDS.items():
        poses[value] = {"hand-left": pt(art(*l)), "hand-right": pt(art(*r))}
    poses["rest"] = {}
    # typing: the hands bob out of step
    typing = anim([("hand-left", 0), ("hand-right", 0.5)], [[0, 0, 110], [0, -1.5, 110]])
    return layer(
        "arms", "arms", "rest", hit=True,
        parts=[{"id": "hand-left", "origin": pt(art(*rest_l)), "shape": "hand"},
               {"id": "hand-right", "origin": pt(art(*rest_r)), "shape": "hand"}],
        shapes={"hand": fill(ellipse(0, 0, k(11), k(9)), SHADE)},
        poses=poses, transition_ms=140,
        animations={"hold": typing})


def prop():
    lid = fill(rect(*art(-36, -43), k(72), k(37), k(6)), METAL)
    logo = fill(ellipse(*art(0, -25), k(5), k(5)), ACCENT, 0.8)
    base = fill(rect(*art(-44, -8), k(88), k(9), k(4)), METAL_DARK)
    ring = (34, -84)
    lens = [
        stroke(tf("M 44 -72 L 50 -62"), METAL, k(6)),
        fill(ellipse(*art(*ring), k(17), k(17)), "#8C98FF40"),
        stroke(ellipse(*art(*ring), k(17), k(17)), ZZ, k(4)),
        stroke(tf("M 25 -92 Q 28 -96 33 -96"), WHITE, k(2.5), 0.8),
    ]
    return layer("prop", "prop", "laptop", hit=False,
                 shapes={"laptop": [lid, logo, base], "lens": lens},
                 fallback={"none": None})


def screen():
    # the screen's light over the lid's top edge, which glows
    edge = stroke(tf("M -32 -43 L 32 -43"), GLOW, k(3))
    return layer("screen", "prop", "laptop", hit=False, glow=True,
                 shapes={"laptop": edge}, fallback={"lens": None, "none": None})


FX = ["zs", "notes", "dots", "alert"]
FX_VALUES = ["none", "dust", "sparkles"] + FX + [f"{v}+sparkles" for v in FX + ["dust"]]


def float_up(rise, steps, step_ms, drift=0.0):
    """Rising and fading out, then back."""
    return keys([(step_ms, {"dx": drift * i / steps, "dy": -rise * i / steps,
                            "opacity": 1 - i / steps}) for i in range(steps)])


def effect(value, parts, shapes, animation):
    """An effects layer that draws only `value` (and `value` with sparkles)."""
    nothing = {v: None for v in FX_VALUES if v not in (value, f"{value}+sparkles")}
    return layer(value, "fx", value, parts=parts, shapes=shapes, animations={value: animation},
                 fallback={f"{value}+sparkles": value, **nothing})


def fx():
    z = lambda s: stroke(f"M {n(-s)} {n(-s)} H {n(s)} L {n(-s)} {n(s)} H {n(s)}", ZZ, 1.6)
    note = [fill(ellipse(-1.6, 3, 2.4, 1.8), ZZ), stroke("M 0.6 3 V -4 L 4 -5", ZZ, 1.3)]
    dot = fill(ellipse(0, 0, 2.2, 2.2), ZZ)
    bang = [stroke("M 0 -6 V 1.5", "#E3AE45", 3), fill(ellipse(0, 6, 1.8, 1.8), "#E3AE45")]
    top = art(0, -122)[1]
    part = lambda i, x, y, shape: {"id": i, "origin": [x, round(y, 2)], "shape": shape}
    return [
        effect("zs", [part("z1", 100, top + 2, "z-small"), part("z2", 106, top - 6, "z"),
                      part("z3", 113, top - 14, "z-big")],
               {"z-small": z(2.2), "z": z(3), "z-big": z(3.8)},
               anim([("z1", 0), ("z2", 0.33), ("z3", 0.66)], float_up(6, 6, 300, 3))),
        effect("notes", [part("note1", 101, top + 4, "note"), part("note2", 27, top + 10, "note")],
               {"note": note}, anim([("note1", 0), ("note2", 0.5)], float_up(8, 8, 180))),
        effect("dots", [part(f"dot{i}", 86 + 8 * i, top - 2, "dot") for i in (1, 2, 3)], {"dot": dot},
               anim([("dot1", 0), ("dot2", 0.33), ("dot3", 0.66)],
                    keys([(240, {"opacity": 1}), (240, {"opacity": 0.6}), (240, {"opacity": 0.25})]))),
        effect("alert", [part("bang", 96, top - 4, "bang")], {"bang": bang},
               anim("bang", keys([(180, {"dy": 0}), (180, {"dy": -1.5})]))),
    ]


def star(r):
    i = r * 0.3
    return fill(f"M 0 {n(-r)} Q {n(i)} {n(-i)} {n(r)} 0 Q {n(i)} {n(i)} 0 {n(r)} Q {n(-i)} {n(i)} {n(-r)} 0 "
                f"Q {n(-i)} {n(-i)} 0 {n(-r)} Z", "#FFF3B0")


def sparkles():
    twinkle = keys([(120, {"scale": 1}), (120, {"scale": 0.7, "opacity": 0.8}), (120, {"scale": 0.4, "opacity": 0.5}),
                    (120, {"scale": 0.7, "opacity": 0.8})])
    return layer(
        "sparkles", "fx", "sparkles",
        parts=[{"id": "spark1", "origin": [22, 34], "shape": "star"},
               {"id": "spark2", "origin": [110, 46], "shape": "star"},
               {"id": "spark3", "origin": [30, 82], "shape": "star-small"}],
        shapes={"star": star(4.5), "star-small": star(3)},
        animations={"sparkles": anim([("spark1", 0), ("spark2", 0.33), ("spark3", 0.66)], twinkle)},
        fallback={v: None for v in ["none", "dust"] + FX})


def dust():
    puff = lambda r: fill(ellipse(0, 0, r, r * 0.8), "#C9C2B8", 0.7)
    feet_y = ANCHOR[1] - 2
    drift = keys([(110, {"dx": -3 * i, "dy": -i * 0.8, "scale": 1 + 0.25 * i, "opacity": 1 - i / 4})
                  for i in range(4)])
    return layer(
        "dust", "fx", "dust", mirror="flip",
        parts=[{"id": "puff1", "origin": [38, feet_y], "shape": "puff"},
               {"id": "puff2", "origin": [30, feet_y - 3], "shape": "puff-small"}],
        shapes={"puff": puff(3.4), "puff-small": puff(2.4)},
        animations={"dust": anim([("puff1", 0), ("puff2", 0.5)], drift)},
        fallback={"dust+sparkles": "dust",
                  **{v: None for v in FX_VALUES if v not in ("dust", "dust+sparkles")}})


# the colours a theme may change, by name
NAMED = {"body": PET, "shade": SHADE, "belly": BELLY, "cheek": CHEEK, "leaf": LEAF, "eyes": EYE}
# Pip in Peach (its own colours) has a Lilac theme, which the painted Pip paints as its sheet theme of that name
LILAC = {"body": "#B9A2F0", "shade": "#9A82D6", "belly": "#DCCFFA", "cheek": "#FF8FC4"}


def pack():
    import vector_pack
    return vector_pack.named({
        "format_version": 1,
        "name": "Pip",
        "kind": "vector",
        "pixel_size": 1,
        "anchor": list(ANCHOR),
        "mirror": "flip",
        "accent": PET,
        "glow": GLOW,
        "layers": [feet(), sprout(), body(), mouth(), eyes(), headphones(), prop(), arms(), screen(), *fx(),
                   sparkles(), dust()],
    }, NAMED, theme="Peach", themes={"Lilac": LILAC})


def main():
    here = os.path.dirname(os.path.abspath(__file__))
    out = os.path.join(here, "..", "..", "rust", "crates", "aipet-sprite", "packs", "pip")
    os.makedirs(out, exist_ok=True)
    with open(os.path.join(out, "pack.json"), "w") as f:
        json.dump(pack(), f, indent=1)
        f.write("\n")


if __name__ == "__main__":
    main()
