using System.Buffers.Binary;
using System.IO.Compression;
using System.Text;

namespace AiPet;

/// A straight-alpha RGBA image, and the two file formats the icons need: PNG and ICO. Written by hand so the tool
/// needs no packages: PNG is zlib (ZLibStream) plus a CRC-32 per chunk, and ICO is a directory of PNGs.
sealed class Image
{
    public readonly int Width, Height;
    public readonly byte[] Rgba;

    public Image(int width, int height) { Width = width; Height = height; Rgba = new byte[width * height * 4]; }

    /// 8-bit RGBA, no interlacing, filter type None on every row (the flat colours of pixel art compress well without).
    public byte[] ToPng()
    {
        var raw = new byte[Height * (Width * 4 + 1)];
        for (int y = 0; y < Height; y++) Array.Copy(Rgba, y * Width * 4, raw, y * (Width * 4 + 1) + 1, Width * 4);
        var idat = new MemoryStream();
        using (var z = new ZLibStream(idat, CompressionLevel.SmallestSize, leaveOpen: true)) z.Write(raw);

        var ihdr = new byte[13];
        BinaryPrimitives.WriteInt32BigEndian(ihdr, Width);
        BinaryPrimitives.WriteInt32BigEndian(ihdr.AsSpan(4), Height);
        ihdr[8] = 8;   // bit depth
        ihdr[9] = 6;   // colour type: RGBA; compression, filter and interlace methods stay 0

        var png = new MemoryStream();
        png.Write(new byte[] { 0x89, (byte)'P', (byte)'N', (byte)'G', 0x0D, 0x0A, 0x1A, 0x0A });
        Chunk(png, "IHDR", ihdr);
        Chunk(png, "IDAT", idat.ToArray());
        Chunk(png, "IEND", Array.Empty<byte>());
        return png.ToArray();
    }

    static void Chunk(Stream s, string type, byte[] data)
    {
        var head = new byte[8];
        BinaryPrimitives.WriteInt32BigEndian(head, data.Length);
        Encoding.ASCII.GetBytes(type, head.AsSpan(4));
        s.Write(head);
        s.Write(data);
        var crc = new byte[4];
        BinaryPrimitives.WriteUInt32BigEndian(crc, Crc32.Update(Crc32.Update(0xFFFFFFFF, head.AsSpan(4)), data) ^ 0xFFFFFFFF);
        s.Write(crc);
    }

    /// A multi-size .ico whose entries are PNGs (Windows Vista and later read those at every size). 256 px is written
    /// as 0 in the one-byte width and height fields.
    public static byte[] ToIco(IReadOnlyList<Image> images)
    {
        var pngs = images.Select(i => i.ToPng()).ToArray();
        var ico = new MemoryStream();
        var w = new BinaryWriter(ico);
        w.Write((ushort)0);                   // reserved
        w.Write((ushort)1);                   // type: icon
        w.Write((ushort)images.Count);
        int offset = 6 + 16 * images.Count;
        for (int i = 0; i < images.Count; i++)
        {
            w.Write((byte)(images[i].Width >= 256 ? 0 : images[i].Width));
            w.Write((byte)(images[i].Height >= 256 ? 0 : images[i].Height));
            w.Write((byte)0);                 // palette size
            w.Write((byte)0);                 // reserved
            w.Write((ushort)1);               // colour planes
            w.Write((ushort)32);              // bits per pixel
            w.Write(pngs[i].Length);
            w.Write(offset);
            offset += pngs[i].Length;
        }
        foreach (var p in pngs) w.Write(p);
        w.Flush();
        return ico.ToArray();
    }
}

/// CRC-32 as PNG uses it (polynomial 0xEDB88320). Callers start from 0xFFFFFFFF and invert the result.
static class Crc32
{
    static readonly uint[] Table = Enumerable.Range(0, 256).Select(n =>
    {
        uint c = (uint)n;
        for (int k = 0; k < 8; k++) c = (c & 1) != 0 ? 0xEDB88320 ^ (c >> 1) : c >> 1;
        return c;
    }).ToArray();

    public static uint Update(uint crc, ReadOnlySpan<byte> data)
    {
        foreach (var b in data) crc = Table[(crc ^ b) & 0xFF] ^ (crc >> 8);
        return crc;
    }
}
