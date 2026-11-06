"""List each frame's chunk types in .aseprite files, plus header flags. Spike helper; run with python3 -I."""
import struct, sys

for path in sys.argv[1:]:
    b = open(path, 'rb').read()
    size, magic, nframes, w, h, depth, flags = struct.unpack_from('<IHHHHHI', b, 0)
    at, out = 128, []
    for fi in range(nframes):
        fsize, fmagic, old, ms, _, new = struct.unpack_from('<IHHHHI', b, at)
        n = new or old
        p, kinds = at + 16, []
        for _ in range(n):
            csize, ctype = struct.unpack_from('<IH', b, p)
            kinds.append('%04x' % ctype)
            p += csize
        out.append('f%d:%s' % (fi, ','.join(kinds)))
        at += fsize
        if fi >= 1:
            break
    print('%s depth=%d flags=%d %s' % (path.split('/')[-1], depth, flags, ' '.join(out)))
