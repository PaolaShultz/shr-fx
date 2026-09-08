#!/usr/bin/env python3
"""Render TestBackend cells using the installed 12x24 PSF font and VT palette.
No audio, MIDI, terminal or operating-system configuration is opened/changed.
Requires Pillow only for this opt-in documentation renderer (not the app).
"""
import gzip
import json
from pathlib import Path
import struct
import subprocess
from PIL import Image

root = Path(__file__).resolve().parent.parent
out = root / "docs" / "screens"
out.mkdir(parents=True, exist_ok=True)
screens = json.loads(subprocess.check_output(
    ["cargo", "run", "--locked", "--example", "screen_gallery"], cwd=root
))
font_path = Path("/usr/share/consolefonts/Uni2-TerminusBold24x12.psf.gz")
font = gzip.decompress(font_path.read_bytes())
magic, version, header, flags, count, size, height, width = struct.unpack_from("<8I", font)
assert magic == 0x864AB572 and (width, height) == (12, 24)
glyphs = {}
for index, aliases in enumerate(font[header+count*size:].split(b"\xff")[:count]):
    for char in aliases.split(b"\xfe")[0].decode("utf-8"):
        glyphs[char] = font[header+index*size:header+(index+1)*size]
# Read the existing console defaults; never write a palette or change tty state.
components = [list(map(int, Path(f"/sys/module/vt/parameters/default_{part}").read_text().strip().split(','))) for part in ("red", "grn", "blu")]
palette = list(zip(*components))

def cell(image, x, y, symbol, fg, bg, bold=False):
    colour = palette[fg+8 if bold and fg < 8 else fg]
    background = palette[bg]
    assert len(symbol) == 1 and symbol in glyphs, f"Unsupported terminal glyph: {symbol!r}"
    data = glyphs[symbol]
    for row in range(24):
        bits = int.from_bytes(data[row*2:row*2+2], "big")
        for col in range(12):
            image.putpixel((x+col,y+row), colour if bits & (1 << (15-col)) else background)

images = []
for screen in screens:
    image = Image.new("RGB", (480,320), palette[0])
    for c in screen["cells"]:
        cell(image,c["x"]*12,c["y"]*24,c["symbol"],c["fg"],c["bg"],c["bold"])
    image.save(out / f"{screen['name']}.png")
    images.append(image)
# A contact sheet preserves the native pixels; each title occupies one font row.
sheet = Image.new("RGB",(984,((len(images)+1)//2)*368),palette[0])
for i,(screen,image) in enumerate(zip(screens,images)):
    x=(i%2)*504; y=(i//2)*368
    for j,char in enumerate(screen['name']):
        cell(sheet,x+j*12,y,char,3,0,True)
    sheet.paste(image,(x,y+24))
sheet.save(out / "gallery.png")
lines = ["# Renderer gallery", "", "Generated from `ui::draw` with ratatui TestBackend at 40×13. PNGs use the installed Uni2-TerminusBold24x12 PSF font, read-only VT default palette, and 480×320 canvas (eight bottom pixels unused). These are offline renderer previews, not photographs or hardware acceptance. The meter example uses explicitly simulated peaks.", "", "Regenerate: `python3 scripts/screen_gallery.py` (Pillow needed for this opt-in renderer). No audio files or user configuration are read or written.", "", "![All screens](gallery.png)", ""]
for screen in screens:
    lines += [f"## {screen['name']}", "", f"![{screen['name']}]({screen['name']}.png)", "", "```text", *[row.rstrip() for row in screen['rows']], "```", ""]
(out / "README.md").write_text('\n'.join(lines))
print(f"Wrote {len(screens)} renderer previews to {out}")
