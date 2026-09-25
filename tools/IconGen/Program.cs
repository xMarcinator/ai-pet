namespace AiPet;

/// IconGen [--root <repo>] [--preview <dir>]
///
/// Draws the app icon with the pet's own renderer and writes every icon file the repo ships. The icon is the default
/// avatar, Sprout, standing idle as the Settings window previews it: the eye layer over the body layer, cropped to
/// the pet and centred on a transparent square, as crisp pixel art (see Sprite.Draw). Run it again when the sprite or
/// Sprout's palette changes, and commit the output; the builds only use the committed files.
///
///   assets/icon/aipet.ico                                  exe and window icon: 16, 20, 24, 32, 40, 48, 64, 256
///   assets/icon/aipet-<n>.png                              the same sizes, plus 128 and 512 (README header)
///   plugins/aipet/assets/icon.png, logo.png                Codex plugin: composer icon (128), logo (512)
///   packaging/linux/icons/hicolor/<n>x<n>/apps/aipet.png   Linux menus and docks, 16 to 512
///
/// The repo root is found from the tool's own folder (the one holding AiPet.slnx) unless --root is given.
/// --preview <dir> also writes <dir>/preview.png: each size at 1:1 on light and dark, and magnified on a checkerboard.
/// Set AIPET_DATA_DIR to an empty folder so the custom avatars in your own data folder aren't read.
static class Program
{
    static readonly int[] IcoSizes = { 16, 20, 24, 32, 40, 48, 64, 256 };
    static readonly int[] PngSizes = { 16, 20, 24, 32, 40, 48, 64, 128, 256, 512 };
    static readonly int[] HicolorSizes = { 16, 22, 24, 32, 48, 64, 128, 256, 512 };

    static int Main(string[] args)
    {
        string root = null, preview = null;
        for (int i = 0; i < args.Length; i++)
        {
            if (args[i] == "--root" && i + 1 < args.Length) root = args[++i];
            else if (args[i] == "--preview" && i + 1 < args.Length) preview = args[++i];
            else { Console.Error.WriteLine("usage: IconGen [--root <repo>] [--preview <dir>]"); return 2; }
        }
        root ??= FindRoot(AppContext.BaseDirectory);
        if (root == null || !File.Exists(Path.Combine(root, "AiPet.slnx")))
        {
            Console.Error.WriteLine("IconGen: can't find the repo root (the folder with AiPet.slnx); pass --root <repo>");
            return 2;
        }

        var sprite = Sprite.IdleSprout();
        Console.WriteLine($"Sprout, idle: {sprite.W}x{sprite.H} cells");
        var icons = new Dictionary<int, Image>();
        Image Icon(int size) => icons.TryGetValue(size, out var i) ? i : icons[size] = sprite.Draw(size);

        void Write(string rel, byte[] data)
        {
            var path = Path.Combine(root, rel);
            Directory.CreateDirectory(Path.GetDirectoryName(path));
            File.WriteAllBytes(path, data);
            Console.WriteLine($"  {rel.Replace('\\', '/')}  ({data.Length:N0} bytes)");
        }

        Write(Path.Combine("assets", "icon", "aipet.ico"), Image.ToIco(IcoSizes.Select(Icon).ToList()));
        foreach (var n in PngSizes) Write(Path.Combine("assets", "icon", $"aipet-{n}.png"), Icon(n).ToPng());
        Write(Path.Combine("plugins", "aipet", "assets", "icon.png"), Icon(128).ToPng());
        Write(Path.Combine("plugins", "aipet", "assets", "logo.png"), Icon(512).ToPng());
        foreach (var n in HicolorSizes)
            Write(Path.Combine("packaging", "linux", "icons", "hicolor", $"{n}x{n}", "apps", "aipet.png"), Icon(n).ToPng());

        if (preview != null)
        {
            var path = Path.Combine(Path.GetFullPath(preview), "preview.png");
            Directory.CreateDirectory(Path.GetDirectoryName(path));
            File.WriteAllBytes(path, Preview(PngSizes.Concat(HicolorSizes).Distinct().Order().Select(Icon).ToList()).ToPng());
            Console.WriteLine($"  preview: {path}");
        }
        return 0;
    }

    static string FindRoot(string dir)
    {
        for (var d = new DirectoryInfo(dir); d != null; d = d.Parent)
            if (File.Exists(Path.Combine(d.FullName, "AiPet.slnx"))) return d.FullName;
        return null;
    }

    /// A sheet for checking the icons by eye: every size up to 128 at 1:1 on a light and on a dark strip, then the
    /// sizes up to 64 magnified to about 256 px (nearest neighbour) on a checkerboard, so the transparency shows.
    static Image Preview(List<Image> icons)
    {
        const int gap = 12, tile = 256 + 2 * gap, perRow = 4;
        var small = icons.Where(i => i.Width <= 128).ToList();
        var zoomed = icons.Where(i => i.Width <= 64).ToList();
        int stripH = 128 + 2 * gap;
        int width = Math.Max(perRow * tile, small.Sum(i => i.Width + gap) + gap);
        int rows = (zoomed.Count + perRow - 1) / perRow;
        var sheet = new Image(width, 2 * stripH + rows * tile);

        Fill(sheet, 0, 0, width, stripH, (_, _) => 0xF3F3F3);
        Fill(sheet, 0, stripH, width, stripH, (_, _) => 0x202020);
        Fill(sheet, 0, 2 * stripH, width, rows * tile, (x, y) => ((x / 16 + y / 16) & 1) == 0 ? 0xFFFFFFu : 0xD6D6D6u);
        for (int strip = 0; strip < 2; strip++)
        {
            int x = gap;
            foreach (var i in small) { Blit(sheet, i, x, strip * stripH + gap + (128 - i.Height) / 2, 1); x += i.Width + gap; }
        }
        for (int n = 0; n < zoomed.Count; n++)
        {
            var i = zoomed[n];
            int m = 256 / i.Width;
            Blit(sheet, i, n % perRow * tile + gap + (256 - m * i.Width) / 2, 2 * stripH + n / perRow * tile + gap + (256 - m * i.Height) / 2, m);
        }
        return sheet;
    }

    static void Fill(Image dst, int x0, int y0, int w, int h, Func<int, int, uint> rgb)
    {
        for (int y = y0; y < y0 + h; y++)
            for (int x = x0; x < x0 + w; x++)
            {
                uint c = rgb(x, y);
                int o = (y * dst.Width + x) * 4;
                dst.Rgba[o] = (byte)(c >> 16); dst.Rgba[o + 1] = (byte)(c >> 8); dst.Rgba[o + 2] = (byte)c; dst.Rgba[o + 3] = 255;
            }
    }

    /// Draws src over an opaque dst at (x0, y0), each pixel as an m×m block.
    static void Blit(Image dst, Image src, int x0, int y0, int m)
    {
        for (int y = 0; y < src.Height * m; y++)
            for (int x = 0; x < src.Width * m; x++)
            {
                int s = ((y / m) * src.Width + x / m) * 4, d = ((y0 + y) * dst.Width + x0 + x) * 4;
                int a = src.Rgba[s + 3];
                for (int c = 0; c < 3; c++) dst.Rgba[d + c] = (byte)((src.Rgba[s + c] * a + dst.Rgba[d + c] * (255 - a) + 127) / 255);
            }
    }
}

/// The pet as a grid of pixels (at first one per cell), cropped to the opaque ones: premultiplied RGBA in 0..1, four
/// values per pixel, plus how much losing each pixel would show when the grid is shrunk (see Carve).
sealed class Sprite
{
    public readonly int W, H;
    readonly double[] px, weight;

    Sprite(int w, int h, double[] px, double[] weight) { W = w; H = h; this.px = px; this.weight = weight; }

    /// Idle Sprout exactly as the Settings window renders its preview: three updates at 0.05 s steps. At that point
    /// the idle clock hasn't started a blink, a glance or an idle move yet, so the pose is always the same.
    public static Sprite IdleSprout()
    {
        // All() resolves the palette; the static Avatar.Sprout has only the hex strings until then
        var avatar = Avatar.All().First(a => a.Name == Avatar.Sprout.Name);
        var pet = new Pet();
        pet.SetAvatar(avatar);
        var input = new PetInput { State = "idle" };
        PetFrame f = null;
        for (int i = 1; i <= 3; i++) f = pet.Update(input, i * 0.05);

        // the eye layer over the body (both premultiplied BGRA, 0xAARRGGBB as uints), then crop to the opaque cells
        var full = new double[Pet.GW * Pet.GH * 4];
        int x0 = Pet.GW, y0 = Pet.GH, x1 = -1, y1 = -1;
        for (int y = 0; y < Pet.GH; y++)
            for (int x = 0; x < Pet.GW; x++)
            {
                int i = y * Pet.GW + x;
                uint body = f.Body[i], glow = f.Glow[i];
                double ga = (glow >> 24) / 255.0;
                for (int c = 0; c < 4; c++)
                {
                    int shift = c == 3 ? 24 : 16 - 8 * c;   // r, g, b, a
                    full[i * 4 + c] = ((glow >> shift) & 255) / 255.0 + ((body >> shift) & 255) / 255.0 * (1 - ga);
                }
                if (full[i * 4 + 3] > 0) { x0 = Math.Min(x0, x); y0 = Math.Min(y0, y); x1 = Math.Max(x1, x); y1 = Math.Max(y1, y); }
            }
        if (x1 < 0) throw new InvalidOperationException("the pet rendered no pixels");

        // weights: the eyes 8, the silhouette's edge (the outline) 3, the face panel 2, the rest of the body 1
        int w = x1 - x0 + 1, h = y1 - y0 + 1;
        var px = new double[w * h * 4];
        var weight = new double[w * h];
        bool Clear(int x, int y) => x is < 0 or >= Pet.GW || y is < 0 or >= Pet.GH || full[(y * Pet.GW + x) * 4 + 3] == 0;
        for (int y = 0; y < h; y++)
        {
            Array.Copy(full, ((y0 + y) * Pet.GW + x0) * 4, px, y * w * 4, w * 4);
            for (int x = 0; x < w; x++)
            {
                int gx = x0 + x, gy = y0 + y;
                bool face = gx is >= 8 and <= 17 && gy >= avatar.FaceTop && gy <= avatar.FaceBottom;   // Avatar's face panel
                weight[y * w + x] = Clear(gx, gy) ? 0 : f.Glow[gy * Pet.GW + gx] >> 24 != 0 ? 8
                    : Clear(gx - 1, gy) || Clear(gx + 1, gy) || Clear(gx, gy - 1) || Clear(gx, gy + 1) ? 3 : face ? 2 : 1;
            }
        }
        return new Sprite(w, h, px, weight);
    }

    /// The pet centred on a transparent size×size square, with a whole number of pixels per cell so every cell stays
    /// a crisp square and no pixel is blended. Where the largest number that fits leaves the pet filling most of the
    /// square, that's all (22 and 24 px at 1×, 48 at 2×, 64 at 3×, 128 at 6×, 256 at 12×, 512 at 24×). The sizes in
    /// between (16, 20, 32, 40) take the next number up and then shrink to fit by dropping the pixel rows and columns
    /// whose loss shows least, keeping the pet's proportions.
    public Image Draw(int size)
    {
        int side = Math.Max(W, H), k = size / side;
        if (k >= 1 && k * side >= 0.85 * size) return Scaled(k).Centred(size);
        var big = Scaled(k + 1);
        double r = (double)size / (side * (k + 1));
        int w = Math.Min(size, (int)Math.Round(big.W * r)), h = Math.Min(size, (int)Math.Round(big.H * r));
        if ((big.W - w) % 2 != 0) w += w < size ? 1 : -1;   // columns go in mirrored pairs
        return big.Carve(w, h).Centred(size);
    }

    /// Nearest-neighbour: each pixel becomes k×k.
    Sprite Scaled(int k)
    {
        var p = new double[W * k * H * k * 4];
        var wt = new double[W * k * H * k];
        for (int y = 0; y < H * k; y++)
            for (int x = 0; x < W * k; x++)
            {
                Array.Copy(px, ((y / k) * W + x / k) * 4, p, (y * W * k + x) * 4, 4);
                wt[y * W * k + x] = weight[(y / k) * W + x / k];
            }
        return new Sprite(W * k, H * k, p, wt);
    }

    /// Shrinks to w×h by dropping whole pixel rows and columns, never the outermost ones, and columns in mirrored pairs
    /// so the pet stays symmetric. Each step drops the line whose loss shows least: every pixel on it shortens the run
    /// of equal pixels it sits in (across the line) by one, which costs the pixel's weight over the run's length, and
    /// the last pixel between two runs of one colour also costs the weight of what then merges (as the two eyes would).
    /// So the doubled lines of an upscale and the wide flat stretches of body go first, and the outline, the face and
    /// above all the eyes last.
    Sprite Carve(int w, int h)
    {
        var rows = Enumerable.Range(0, H).ToList();
        var cols = Enumerable.Range(0, W).ToList();
        bool Same(int a, int b) =>
            px[a * 4] == px[b * 4] && px[a * 4 + 1] == px[b * 4 + 1] && px[a * 4 + 2] == px[b * 4 + 2] && px[a * 4 + 3] == px[b * 4 + 3];
        // what dropping line j of lines (rows when row, else columns) costs, across the kept lines of the other kind
        double Cost(List<int> lines, int j, bool row)
        {
            double cost = 0;
            foreach (var o in row ? cols : rows)
            {
                int At(int i) => row ? lines[i] * W + o : o * W + lines[i];
                int c = At(j);
                if (weight[c] == 0) continue;
                int run = 1;
                for (int i = j - 1; i >= 0 && Same(At(i), c); i--) run++;
                for (int i = j + 1; i < lines.Count && Same(At(i), c); i++) run++;
                cost += weight[c] / run;
                if (run == 1 && j > 0 && j < lines.Count - 1 && Same(At(j - 1), At(j + 1))) cost += weight[At(j - 1)];
            }
            return cost;
        }
        while (cols.Count > w)
        {
            int n = cols.Count, best = 1;
            double bestCost = double.MaxValue;
            for (int j = 1; j < n / 2; j++)
            {
                var without = cols.ToList();
                without.RemoveAt(n - 1 - j);
                double c = Cost(cols, n - 1 - j, false) + Cost(without, j, false);
                if (c < bestCost) { bestCost = c; best = j; }
            }
            cols.RemoveAt(n - 1 - best);
            cols.RemoveAt(best);
        }
        while (rows.Count > h)
        {
            int best = 1;
            double bestCost = double.MaxValue;
            for (int j = 1; j < rows.Count - 1; j++)
            {
                double c = Cost(rows, j, true);
                if (c < bestCost) { bestCost = c; best = j; }
            }
            rows.RemoveAt(best);
        }
        var p = new double[w * h * 4];
        var wt = new double[w * h];
        for (int y = 0; y < h; y++)
            for (int x = 0; x < w; x++)
            {
                Array.Copy(px, (rows[y] * W + cols[x]) * 4, p, (y * w + x) * 4, 4);
                wt[y * w + x] = weight[rows[y] * W + cols[x]];
            }
        return new Sprite(w, h, p, wt);
    }

    /// Onto a transparent size×size square, centred (an odd pixel of margin goes right or below), as straight
    /// (not premultiplied) RGBA.
    Image Centred(int size)
    {
        var img = new Image(size, size);
        int ox = (size - W) / 2, oy = (size - H) / 2;
        for (int y = 0; y < H; y++)
            for (int x = 0; x < W; x++)
            {
                int i = (y * W + x) * 4, o = ((oy + y) * size + ox + x) * 4;
                double a = px[i + 3];
                if (a <= 0.5 / 255) continue;   // fully transparent stays 0,0,0,0
                for (int c = 0; c < 3; c++) img.Rgba[o + c] = ToByte(px[i + c] / a);
                img.Rgba[o + 3] = ToByte(a);
            }
        return img;
    }

    static byte ToByte(double v) => (byte)Math.Round(Math.Clamp(v, 0, 1) * 255);
}
