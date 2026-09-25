using System.Globalization;
using System.Text.Json;

namespace AiPet;

/// A pet look: silhouette, palette, shading style and eye glow. All avatars share the 26×24 pixel grid
/// and the same anchor points (face at x 8–17 / y 9–14, eyes on rows 10–13, arms and feet at the sides
/// and bottom), so every animation works with every avatar.
///
/// Custom avatars: drop a JSON file in the avatars folder (Paths.DataDir/avatars) with the same fields as
/// the built-ins below (colours as "#RRGGBB" or "#AARRGGBB", shapes as [cx, cy, rx, ry] ellipses).
/// palette.accent is the avatar's theme colour (menu check marks, send button).
public sealed class Avatar
{
    public string Name { get; set; }
    /// Silhouette as a union of ellipses on the pixel grid: [cx, cy, rx, ry].
    public double[][] Shapes { get; set; }
    /// "soft": highlight top-left, shade bottom-right (a lit toy). "rim": dark body with a glowing edge.
    public string Shading { get; set; } = "soft";
    public bool Blush { get; set; } = true;
    /// Face panel rows (inclusive); columns are always 8–17.
    public int FaceTop { get; set; } = 9;
    public int FaceBottom { get; set; } = 14;
    public Dictionary<string, string> Palette { get; set; } = new();

    // resolved colours (0xAARRGGBB)
    public uint Outline, Deep, Shade, Body, Hi, Spec, BlushC, Screen, Bezel, Glint, Eye, EyeHi, EyeLo, Glow, Accent;

    public static readonly Avatar Sprout = new()
    {
        Name = "Sprout",
        Shapes = new[] { new[] { 13, 14, 7.3, 7.3 }, new[] { 8.6, 9.2, 4.0, 4.0 }, new[] { 13, 7.2, 4.6, 4.6 }, new[] { 17.4, 9.2, 4.0, 4.0 } },
        Shading = "soft", Blush = true,
        Palette = new()
        {
            ["outline"] = "#3E1C12", ["deep"] = "#A24A2C", ["shade"] = "#C65F3B", ["body"] = "#E47E55", ["hi"] = "#F5A57D",
            ["spec"] = "#FFD6BC", ["blush"] = "#F08A7C", ["screen"] = "#171A2C", ["bezel"] = "#2A2F4D", ["glint"] = "#3D4674",
            ["eye"] = "#7FEFFF", ["eyeHi"] = "#E4FFFF", ["eyeLo"] = "#3FB8D8", ["glow"] = "#7FEFFF", ["accent"] = "#E27A52",
        },
    };

    /// A hooded figure in the dark: rim-lit blue hood, an empty black face and two glowing eyes.
    public static readonly Avatar Hood = new()
    {
        Name = "Hood",
        Shapes = new[]
        {
            new[] { 13, 10.6, 7.2, 7.4 },   // hood
            new[] { 13, 4.4, 3.2, 3.0 },    // hood peak
            new[] { 13, 19.6, 10.2, 4.3 },  // shoulders / cloak
        },
        Shading = "rim", Blush = false, FaceTop = 8, FaceBottom = 15,
        Palette = new()
        {
            ["outline"] = "#04050B", ["deep"] = "#0A1330", ["shade"] = "#0F1C45", ["body"] = "#16285E", ["hi"] = "#3E74E8",
            ["spec"] = "#8DBBFF", ["blush"] = "#16285E", ["screen"] = "#030409", ["bezel"] = "#070A16", ["glint"] = "#070A16",
            ["eye"] = "#66C8FF", ["eyeHi"] = "#E8F7FF", ["eyeLo"] = "#2F8FF0", ["glow"] = "#3D9BFF", ["accent"] = "#4A8CFF",
        },
    };

    public static IReadOnlyList<Avatar> BuiltIn { get; } = new[] { Sprout, Hood };

    public static string CustomDir => Path.Combine(Paths.DataDir, "avatars");

    /// Built-ins plus any valid JSON avatars in the custom folder.
    public static List<Avatar> All()
    {
        var list = BuiltIn.ToList();
        try
        {
            if (Directory.Exists(CustomDir))
                foreach (var f in Directory.GetFiles(CustomDir, "*.json"))
                {
                    try
                    {
                        var a = JsonSerializer.Deserialize<Avatar>(File.ReadAllText(f), new JsonSerializerOptions { PropertyNameCaseInsensitive = true });
                        if (a?.Shapes is { Length: > 0 })
                        {
                            a.Name ??= Path.GetFileNameWithoutExtension(f);
                            list.Add(a);
                        }
                    }
                    catch { /* skip broken files */ }
                }
        }
        catch { }
        foreach (var a in list) a.Resolve();
        return list;
    }

    /// "#RRGGBB" or "#AARRGGBB" -> 0xAARRGGBB.
    public static bool TryParseColor(string hex, out uint argb)
    {
        argb = 0;
        if (string.IsNullOrWhiteSpace(hex)) return false;
        var h = hex.Trim().TrimStart('#');
        if (h.Length == 6) h = "FF" + h;
        return h.Length == 8 && uint.TryParse(h, NumberStyles.HexNumber, CultureInfo.InvariantCulture, out argb);
    }

    void Resolve()
    {
        uint C(string key, uint fallback) =>
            Palette != null && Palette.TryGetValue(key, out var hex) && TryParseColor(hex, out var c) ? c : fallback;
        Outline = C("outline", 0xFF3E1C12); Deep = C("deep", 0xFFA24A2C); Shade = C("shade", 0xFFC65F3B);
        Body = C("body", 0xFFE47E55); Hi = C("hi", 0xFFF5A57D); Spec = C("spec", 0xFFFFD6BC); BlushC = C("blush", 0xFFF08A7C);
        Screen = C("screen", 0xFF171A2C); Bezel = C("bezel", 0xFF2A2F4D); Glint = C("glint", 0xFF3D4674);
        Eye = C("eye", 0xFF7FEFFF); EyeHi = C("eyeHi", 0xFFE4FFFF); EyeLo = C("eyeLo", 0xFF3FB8D8);
        Glow = C("glow", Eye);
        // theme colour for check marks, the send button and other accents; defaults to the rim/highlight colour
        Accent = C("accent", Hi);
    }
}
