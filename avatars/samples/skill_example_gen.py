#!/usr/bin/env python3
"""Writes the worked example of the aipet-avatar-pack skill: avatars/samples/skill-example/pack.json (Dewdrop, a
small vector pet) and avatars/samples/skill-example-preview.html (its review page).

Dewdrop is a water drop with a body that squashes when it sleeps, eyes that follow the pointer, two hands that rest,
bob, wave and lift, Zs while it sleeps and sparkles when it's done or poked. Its colours are a palette with two
themes, Rain (the default) and Dusk. Every other state falls back to what it has: see
.claude/skills/aipet-avatar-pack/references/worked-example.md, which walks through this file.

The drawing follows the owner's conventions: a thick outline, flat cel shading (one shade band), a rim light and a
highlight; plain paths with flat fills, no blur, filters, masks or gradients. Unlike the reference generators
(avatars/reference/*_gen.py), which draw SVG and convert it with vector_pack.py, this one writes the manifest's
shapes directly, and draws the review page from the manifest it wrote, so the page is exactly the pack.

Run it from anywhere: python3 avatars/samples/skill_example_gen.py
"""

import json
import math
import os

HERE = os.path.dirname(os.path.abspath(__file__))
PACK = os.path.join(HERE, "skill-example")
PREVIEW = os.path.join(HERE, "skill-example-preview.html")

ANCHOR = (65, 110)  # the ground, where every built-in vector pet stands
CX = 65

PALETTE = {  # the default theme, Rain
    "outline": "#123C4A", "body": "#5CC8E0", "shade": "#3A9FBF", "rim": "#C8F4FF", "eyes": "#0E2530",
    "accent": "#3A9FBF", "glow": "#9FEAFF",
}
DUSK = {"outline": "#2E1F4A", "body": "#B08CE0", "shade": "#8A66C2", "rim": "#EBDDFF", "accent": "#8A66C2",
        "glow": "#E0C8FF"}


def n(v):
    """A number as the manifest writes it: at most 2 decimals, no trailing zeros."""
    s = f"{v:.2f}".rstrip("0").rstrip(".")
    return "0" if s in ("-0", "") else s


def ellipse(cx, cy, rx, ry):
    """An ellipse as path data: two half arcs."""
    return (f"M {n(cx - rx)} {n(cy)} A {n(rx)} {n(ry)} 0 1 0 {n(cx + rx)} {n(cy)} "
            f"A {n(rx)} {n(ry)} 0 1 0 {n(cx - rx)} {n(cy)} Z")


def drop(top, rx, ry):
    """The body for one pose: a drop with its tip at `top` and a round bottom of radii rx, ry standing on the ground.
    Drawn bottom to top: the fill, the shade band, the rim light, the highlight, then the outline over them all, so
    the outline hides where the inner shapes meet the edge (no clip paths needed)."""
    cy = ANCHOR[1] - ry
    outline = (f"M {n(CX)} {n(top)} C {n(CX + rx / 2)} {n(top + 20)} {n(CX + rx)} {n(cy - 20)} {n(CX + rx)} {n(cy)} "
               f"A {n(rx)} {n(ry)} 0 1 1 {n(CX - rx)} {n(cy)} "
               f"C {n(CX - rx)} {n(cy - 20)} {n(CX - rx / 2)} {n(top + 20)} {n(CX)} {n(top)} Z")

    def on(deg):  # a point on the round bottom
        a = math.radians(deg)
        return CX + rx * math.cos(a), cy + ry * math.sin(a)

    (x1, y1), (x2, y2) = on(160), on(30)
    shade = (f"M {n(x1)} {n(y1)} A {n(rx)} {n(ry)} 0 0 0 {n(x2)} {n(y2)} "
             f"Q {n(CX + 1)} {n(cy + ry * 0.73)} {n(x1)} {n(y1)} Z")
    rim = (f"M {n(CX + 7)} {n(top + 12)} C {n(CX + rx * 0.55)} {n(top + 24)} {n(CX + rx * 0.83)} {n(cy - 14)} "
           f"{n(CX + rx * 0.83)} {n(cy - 4)}")
    return [
        {"d": outline, "fill": "$body"},
        {"d": shade, "fill": "$shade"},
        {"d": rim, "stroke": "$rim", "stroke_width": 3, "linecap": "round"},
        {"d": ellipse(CX - rx * 0.5, cy - 8, 3, 5), "fill": "$rim", "opacity": 0.9},
        {"d": outline, "stroke": "$outline", "stroke_width": 4, "linejoin": "round"},
    ]


def eye(rx, ry):
    """An open eye around (0, 0): the pupil and a glint."""
    return [{"d": ellipse(0, 0, rx, ry), "fill": "$eyes"},
            {"d": ellipse(-rx * 0.35, -ry * 0.4, rx * 0.35, rx * 0.35), "fill": "#FFFFFF"}]


def line(d):
    return {"d": d, "stroke": "$eyes", "stroke_width": 2.2, "linecap": "round"}


NEAR, FAR, UP = 3, 5, 2
LOOK = {
    "center": [0, 0], "left": [-NEAR, 0], "right": [NEAR, 0], "up": [0, -UP], "down": [0, UP],
    "up-left": [-NEAR, -UP], "up-right": [NEAR, -UP], "down-left": [-NEAR, UP], "down-right": [NEAR, UP],
    "far-left": [-FAR, 0], "far-right": [FAR, 0],
    "up-far-left": [-FAR, -UP], "up-far-right": [FAR, -UP], "down-far-left": [-FAR, UP], "down-far-right": [FAR, UP],
}
EFFECTS = ["none", "zs", "dots", "notes", "alert", "sparkles", "dust"]


def pack():
    body = {
        "name": "body", "channel": "state", "hit": True, "default": "idle",
        "shapes": {"idle": drop(30, 30, 30), "sleep": drop(44, 34, 25)},
    }
    eyes = {
        "name": "eyes", "channel": "eyes", "hit": True, "glow": False, "default": "open",
        # blink and asleep are the same line; poked squints into the happy arc; up and down look with `look`
        "fallback": {"blink": "closed", "squint": "happy", "up": "open", "down": "open"},
        "parts": [{"id": "eye-left", "origin": [56, 80]}, {"id": "eye-right", "origin": [74, 80]}],
        "shapes": {
            "open": eye(3.5, 5),
            "wide": eye(4.5, 6),
            "closed": line("M -4 1 Q 0 3 4 1"),
            "happy": line("M -4 2 Q 0 -3 4 2"),
        },
        "look": LOOK,
    }
    hand = {"d": ellipse(0, 0, 6, 5), "fill": "$body", "stroke": "$outline", "stroke_width": 3}
    arms = {
        "name": "arms", "channel": "arms", "hit": True, "default": "rest",
        "fallback": {"hold": "rest", "lens": "rest", "run-a": "raised", "run-b": "raised"},
        "parts": [{"id": "hand-left", "origin": [33, 92], "shape": "hand"},
                  {"id": "hand-right", "origin": [97, 92], "shape": "hand"}],
        "shapes": {"hand": hand},
        "poses": {
            "rest": {},
            "raised": {"hand-left": [24, 64], "hand-right": [106, 64]},
            "wave-a": {"hand-right": [106, 60]},
            "wave-b": {"hand-right": [111, 72]},
        },
        "transition_ms": 140,
        "animations": {
            # the hands bob out of step while they rest
            "rest": {"parts": ["hand-left", "hand-right"], "phase": [0, 0.5], "loop": "loop",
                     "keys": [[0, 0, 500], [0, -1, 250], [0, -1.5, 500], [0, -1, 250]]},
        },
    }
    z = {"d": "M -3 -3 H 3 L -3 3 H 3", "stroke": "#E3E8FF", "stroke_width": 1.8, "linecap": "round",
         "linejoin": "round"}
    zs = {
        "name": "zs", "channel": "fx", "default": "zs",
        "fallback": {"zs+sparkles": "zs", **{v: None for v in EFFECTS if v != "zs"}},
        "parts": [{"id": "z-1", "origin": [98, 44], "shape": "z"}, {"id": "z-2", "origin": [107, 33], "shape": "z"}],
        "shapes": {"z": [{**z, "stroke": "$outline", "stroke_width": 3.6}, z]},
        "animations": {
            "zs": {"parts": ["z-1", "z-2"], "phase": [0, 0.5], "loop": "loop",
                   "keys": [{"dy": 0, "ms": 300}, {"dx": 1, "dy": -3, "opacity": 0.75, "ms": 300},
                            {"dx": 2, "dy": -6, "opacity": 0.45, "ms": 300}]},
        },
    }
    star = ("M 0 -7 Q 1 -1 7 0 Q 1 1 0 7 Q -1 1 -7 0 Q -1 -1 0 -7 Z")
    sparkles = {
        "name": "sparkles", "channel": "fx", "default": "sparkles",
        # zs+sparkles and the other combined effects fall back to their last part, sparkles, by themselves
        "fallback": {v: None for v in EFFECTS if v != "sparkles"},
        "parts": [{"id": "spark-1", "origin": [22, 52], "shape": "star"},
                  {"id": "spark-2", "origin": [38, 26], "shape": "star"}],
        "shapes": {"star": {"d": star, "fill": "#FFE27A", "stroke": "$outline", "stroke_width": 1.5,
                            "linejoin": "round"}},
        "animations": {
            "sparkles": {"parts": ["spark-1", "spark-2"], "phase": [0, 0.5], "loop": "pingpong",
                         "keys": [{"scale": 0.6, "ms": 120}, {"scale": 0.85, "ms": 120},
                                  {"scale": 1, "rotate": 15, "ms": 120}]},
        },
    }
    return {
        "format_version": 1,
        "name": "Dewdrop (example)",
        "kind": "vector",
        "pixel_size": 1,
        "anchor": list(ANCHOR),
        "mirror": "flip",
        "accent": "$accent",
        "glow": "$glow",
        "palette": PALETTE,
        "theme": "Rain",
        "themes": {"Dusk": DUSK},
        "layers": [body, eyes, arms, zs, sparkles],
    }


# ------------------------------------------------------------------ the review page, drawn from the manifest

def resolve(layer, value):
    """The value a layer shows, as the app works it out (pack.rs, Layer::resolve): itself, its fallbacks, the last
    part of a combined value, then the default. None draws nothing."""
    have = set(layer.get("shapes", {})) | set(layer.get("poses", {})) | set(layer.get("animations", {}))
    v = value
    for _ in range(9):
        if v in have:
            return v
        fb = layer.get("fallback", {})
        if v in fb:
            if fb[v] is None:
                return None
            v = fb[v]
            continue
        break
    if "+" in v:
        return resolve(layer, v.rsplit("+", 1)[1])
    return layer["default"]


def paint(c, colours):
    return colours[c[1:]] if c.startswith("$") else c


def path(s, colours, transform=""):
    t = f' transform="{transform}"' if transform else ""
    return (f'<path d="{s["d"]}" fill="{paint(s.get("fill", "none"), colours)}" '
            f'stroke="{paint(s.get("stroke", "none"), colours)}" stroke-width="{s.get("stroke_width", 1)}" '
            f'stroke-linecap="{s.get("linecap", "butt")}" stroke-linejoin="{s.get("linejoin", "miter")}" '
            f'opacity="{s.get("opacity", 1)}"{t}/>')


def draw(p, values, colours, look="center"):
    """One frame of the pack for `values` (channel to value), without the app's whole-body motion and first key
    frames only: what the pet shows when that state starts."""
    out = []
    for layer in p["layers"]:
        v = resolve(layer, values.get(layer["channel"], {"state": "idle", "eyes": "open", "arms": "rest",
                                                          "feet": "stand", "prop": "none", "headphones": "off",
                                                          "fx": "none"}[layer["channel"]]))
        if v is None:
            continue
        shapes = layer.get("shapes", {})
        parts = layer.get("parts", [])
        if not parts:
            for s in shapes.get(v, []) if isinstance(shapes.get(v), list) else [shapes[v]] if v in shapes else []:
                out.append(path(s, colours))
            continue
        pose = layer.get("poses", {}).get(v, {})
        for part in parts:
            x, y = pose.get(part["id"], part["origin"])
            if layer["channel"] == "eyes":
                dx, dy = layer.get("look", {}).get(look, [0, 0])
                x, y = x + dx, y + dy
            drawing = shapes.get(part.get("shape", v))
            if drawing is None:
                continue
            for s in drawing if isinstance(drawing, list) else [drawing]:
                out.append(path(s, colours, f"translate({n(x)} {n(y)})"))
    return "".join(out)


REVIEW = [
    ("idle", {}, "center"),
    ("sleep (body sleep, eyes closed, Zs)", {"state": "sleep", "eyes": "closed", "fx": "zs"}, "center"),
    ("sleep, poked (zs+sparkles)", {"state": "sleep", "eyes": "closed", "fx": "zs+sparkles"}, "center"),
    ("thinking (eyes up: open, looking up)", {"state": "thinking", "eyes": "up", "fx": "dots"}, "up"),
    ("working (laptop: hands rest)", {"state": "working", "eyes": "down", "arms": "hold", "prop": "laptop"}, "down"),
    ("attention (wave A)", {"state": "attention", "arms": "wave-a", "fx": "alert"}, "center"),
    ("done (happy, wave B, sparkles)", {"state": "done", "eyes": "happy", "arms": "wave-b", "fx": "sparkles"},
     "center"),
    ("listen (happy, notes: not drawn)", {"state": "listen", "eyes": "happy", "headphones": "on", "fx": "notes"},
     "center"),
    ("poked (squint: happy)", {"eyes": "squint", "fx": "sparkles"}, "center"),
    ("held (wide, raised)", {"eyes": "wide", "arms": "raised", "feet": "dangle-a"}, "center"),
    ("dragged (run: raised)", {"arms": "run-a", "feet": "run-a", "fx": "dust"}, "far-right"),
    ("blink (closed)", {"eyes": "blink"}, "center"),
    ("looking up-left", {}, "up-left"),
    ("looking far-left", {}, "far-left"),
]


def svg(content, mirror=False):
    if mirror:
        content = f'<g transform="translate(130 0) scale(-1 1)">{content}</g>'
    return (f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 130 120" width="130" height="120">'
            f'<ellipse cx="65" cy="111" rx="30" ry="3.5" fill="#000" opacity="0.18"/>{content}</svg>')


def preview(p):
    bgs = [("white", "#ffffff"), ("mid-grey", "#808080"), ("near-black", "#101214")]
    themes = [(p["theme"], p["palette"])] + [(t, {**p["palette"], **cs}) for t, cs in p["themes"].items()]
    sections = []
    for theme, colours in themes:
        rows = []
        for name, values, look in REVIEW + [("mirrored (facing left)", {"facing": "left"}, "right")]:
            frame = svg(draw(p, values, colours, look), mirror=values.get("facing") == "left")
            rows.append(f"<tr><th>{name}</th>" + "".join(
                f'<td><div class="box" style="background:{bg}">{frame}</div></td>' for _, bg in bgs) + "</tr>")
        sections.append(f"<h2>{p['name']} in {theme}</h2><table><tr><th></th>"
                        + "".join(f"<th style='text-align:left'>{b}</th>" for b, _ in bgs) + "</tr>"
                        + "".join(rows) + "</table>")
    return f"""<!doctype html>
<html><head><meta charset="utf-8"><title>{p['name']} review</title>
<style>
  :root {{ color-scheme: light; }}
  body {{ font: 14px system-ui, sans-serif; margin: 16px; background: #f4f4f4; color: #111; }}
  h2 {{ margin: 24px 0 8px; font-size: 16px; }}
  th {{ text-align: right; padding: 4px 10px; font-weight: 500; white-space: nowrap; }}
  td {{ padding: 4px; }}
  .box {{ width: 130px; height: 120px; outline: 1px dashed #bbb; }}
  .box svg {{ display: block; }}
</style></head><body>
<p>Generated by {os.path.basename(__file__)} from the pack it writes. Each state in a 130×120 logical px box
(the art box), on white, mid-grey and near-black; first frames only, without the app's whole-body motion.</p>
{"".join(sections)}
</body></html>
"""


def main():
    p = pack()
    os.makedirs(PACK, exist_ok=True)
    with open(os.path.join(PACK, "pack.json"), "w", encoding="utf-8", newline="\n") as f:
        json.dump(p, f, indent=1)
        f.write("\n")
    with open(PREVIEW, "w", encoding="utf-8", newline="\n") as f:
        f.write(preview(p))


if __name__ == "__main__":
    main()
