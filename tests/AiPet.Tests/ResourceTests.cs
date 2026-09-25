using System.Diagnostics;
using System.IO;
using System.Reflection.Metadata;
using System.Reflection.PortableExecutable;
using System.Text;
using Xunit;

namespace AiPet.Tests;

/// The Windows resources build/Win32Resources.targets gives AiPet.dll and aipet-hook.dll: the icon, the manifest, and
/// a version resource that names the .exe. Windows copies them into AiPet.exe (the apphost) and aipet-hook.exe
/// (NativeAOT); that part only happens on Windows, where FileVersionInfo also reads them the way Windows does.
public class ResourceTests
{
    const int RT_ICON = 3, RT_GROUP_ICON = 14, RT_VERSION = 16, RT_MANIFEST = 24;

    /// The app's dll as the solution build left it, in the tests' own configuration (the tests don't reference it).
    public static string AppDll
    {
        get
        {
            var tfm = new DirectoryInfo(AppContext.BaseDirectory);
            var path = Path.Combine(Scripts.Repo, "src", "AiPet.UI", "bin", tfm.Parent.Name, tfm.Name, "AiPet.dll");
            Assert.True(File.Exists(path), $"{path} isn't there: build the solution (AiPet.slnx), not only the tests");
            return path;
        }
    }

    public static IEnumerable<object[]> Exes => new[]
    {
        new object[] { "app", "AiPet.exe", "AiPet" },
        new object[] { "hook", "aipet-hook.exe", "AiPet hook" },
    };

    static string Dll(string which) => which == "app" ? AppDll : TestEnv.HookDll;

    [Theory]
    [MemberData(nameof(Exes))]
    public void Version_NamesTheExe(string which, string exe, string title)
    {
        var dll = Dll(which);
        var info = VersionInfo(Resources(dll)[(RT_VERSION, 1)]);
        var attributes = Attributes(dll);

        Assert.Equal(exe, info.Strings["InternalName"]);
        Assert.Equal(exe, info.Strings["OriginalFilename"]);
        Assert.Equal(title, info.Strings["FileDescription"]);
        Assert.Equal("AiPet", info.Strings["ProductName"]);
        Assert.Equal("xMarcinator", info.Strings["CompanyName"]);
        Assert.StartsWith("Copyright (c) ", info.Strings["LegalCopyright"]);
        // the version given to the build (-p:Version, else Directory.Build.props), as the assembly has it
        Assert.Equal(attributes["AssemblyInformationalVersionAttribute"], info.Strings["ProductVersion"]);
        Assert.Equal(attributes["AssemblyFileVersionAttribute"], info.Strings["FileVersion"]);
        Assert.Equal(attributes["AssemblyFileVersionAttribute"], info.FileVersion);
        Assert.Equal(title, attributes["AssemblyTitleAttribute"]);
    }

    /// The icon group lists every image of assets/icon/aipet.ico, each an RT_ICON with the image's bytes.
    [Theory]
    [InlineData("app")]
    [InlineData("hook")]
    public void Icon_IsTheAppIcon(string which)
    {
        var res = Resources(Dll(which));
        var ico = File.ReadAllBytes(Path.Combine(Scripts.Repo, "assets", "icon", "aipet.ico"));
        var group = res[(RT_GROUP_ICON, 32512)];
        int count = BitConverter.ToUInt16(ico, 4);
        Assert.True(count > 0);
        Assert.Equal(ico[..6], group[..6]);
        Assert.Equal(6 + 14 * count, group.Length);
        for (int i = 0; i < count; i++)
        {
            int e = 6 + 16 * i, g = 6 + 14 * i;
            Assert.Equal(ico[e..(e + 12)], group[g..(g + 12)]);
            int id = BitConverter.ToUInt16(group, g + 12);
            int size = BitConverter.ToInt32(ico, e + 8), offset = BitConverter.ToInt32(ico, e + 12);
            Assert.Equal(ico[offset..(offset + size)], res[(RT_ICON, id)]);
        }
        Assert.Equal(count, res.Keys.Count(k => k.Type == RT_ICON));
    }

    /// The manifest the compiler would have embedded: the app's app.manifest (per-monitor DPI awareness), the
    /// compiler's default for the hook (asInvoker).
    [Theory]
    [InlineData("app", "PerMonitorV2")]
    [InlineData("hook", "level=\"asInvoker\"")]
    public void Manifest_IsEmbedded(string which, string text) =>
        Assert.Contains(text, Encoding.UTF8.GetString(Resources(Dll(which))[(RT_MANIFEST, 1)]));

    /// Windows reads the version resource as the tests do. The apphost, when the build made one (Windows builds do,
    /// unless UseAppHost=false), has the dll's resources.
    [Theory]
    [MemberData(nameof(Exes))]
    public void Windows_ReadsTheVersion(string which, string exe, string title)
    {
        if (!OperatingSystem.IsWindows()) return;  // elsewhere FileVersionInfo reads the assembly's attributes instead
        var dll = Dll(which);
        var files = new List<string> { dll };
        var apphost = Path.Combine(Path.GetDirectoryName(dll), exe);
        if (which == "app" && File.Exists(apphost)) files.Add(apphost);
        foreach (var f in files)
        {
            var v = FileVersionInfo.GetVersionInfo(f);
            Assert.Equal(exe, v.OriginalFilename);
            Assert.Equal(exe, v.InternalName);
            Assert.Equal(title, v.FileDescription);
            Assert.Equal("AiPet", v.ProductName);
            Assert.Equal(Attributes(dll)["AssemblyInformationalVersionAttribute"], v.ProductVersion);
        }
    }

    // ------------------------------------------------------------------ reading them
    /// The Win32 resources of a PE file by type and numeric name (the first language of each).
    static Dictionary<(int Type, int Name), byte[]> Resources(string file)
    {
        using var pe = new PEReader(File.OpenRead(file));
        var dir = pe.PEHeaders.PEHeader.ResourceTableDirectory;
        Assert.True(dir.Size > 0, $"{file} has no Win32 resources");
        var rsrc = pe.GetSectionData(dir.RelativeVirtualAddress).GetContent(0, dir.Size).ToArray();
        var result = new Dictionary<(int, int), byte[]>();
        IEnumerable<(uint Id, uint Target)> Entries(int at)
        {
            int n = BitConverter.ToUInt16(rsrc, at + 12) + BitConverter.ToUInt16(rsrc, at + 14);
            for (int i = 0; i < n; i++)
                yield return (BitConverter.ToUInt32(rsrc, at + 16 + 8 * i), BitConverter.ToUInt32(rsrc, at + 20 + 8 * i));
        }
        const uint Sub = 0x80000000;
        foreach (var (type, types) in Entries(0).Where(e => (e.Id & Sub) == 0 && (e.Target & Sub) != 0))
            foreach (var (name, names) in Entries((int)(types & ~Sub)).Where(e => (e.Id & Sub) == 0 && (e.Target & Sub) != 0))
            {
                var (_, leaf) = Entries((int)(names & ~Sub)).First();
                int rva = BitConverter.ToInt32(rsrc, (int)leaf), size = BitConverter.ToInt32(rsrc, (int)leaf + 4);
                result[((int)type, (int)name)] = pe.GetSectionData(rva).GetContent(0, size).ToArray();
            }
        return result;
    }

    sealed record Version(string FileVersion, Dictionary<string, string> Strings);

    /// VS_VERSIONINFO: the fixed file version, and the strings of its string table.
    static Version VersionInfo(byte[] d)
    {
        static int Align(int p) => (p + 3) & ~3;
        // a block: length, value length, type (1: text), key, value, children
        (string Key, byte[] Value, List<int> Children) Block(int at)
        {
            int length = BitConverter.ToUInt16(d, at), valueLength = BitConverter.ToUInt16(d, at + 2), type = BitConverter.ToUInt16(d, at + 4);
            int p = at + 6;
            var key = new StringBuilder();
            for (; BitConverter.ToUInt16(d, p) != 0; p += 2) key.Append((char)BitConverter.ToUInt16(d, p));
            p = Align(p + 2);
            int size = type == 1 ? valueLength * 2 : valueLength;
            var value = d[p..(p + size)];
            var children = new List<int>();
            for (p = Align(p + size); p < at + length; p = Align(p + BitConverter.ToUInt16(d, p))) children.Add(p);
            return (key.ToString(), value, children);
        }
        var root = Block(0);
        Assert.Equal("VS_VERSION_INFO", root.Key);
        Assert.Equal(0xFEEF04BDu, BitConverter.ToUInt32(root.Value, 0));
        ushort Word(int i) => BitConverter.ToUInt16(root.Value, 8 + i * 2);
        var fileVersion = $"{Word(1)}.{Word(0)}.{Word(3)}.{Word(2)}";  // dwFileVersionMS, LS: high word first

        var strings = new Dictionary<string, string>();
        foreach (var fileInfo in root.Children.Select(Block).Where(b => b.Key == "StringFileInfo"))
            foreach (var table in fileInfo.Children.Select(Block))
                foreach (var s in table.Children.Select(Block))
                    strings[s.Key] = Encoding.Unicode.GetString(s.Value).TrimEnd('\0');
        return new Version(fileVersion, strings);
    }

    /// The assembly's own string attributes (AssemblyTitleAttribute, ...) by type name.
    static Dictionary<string, string> Attributes(string dll)
    {
        using var pe = new PEReader(File.OpenRead(dll));
        var md = pe.GetMetadataReader();
        var result = new Dictionary<string, string>();
        foreach (var handle in md.GetAssemblyDefinition().GetCustomAttributes())
        {
            var a = md.GetCustomAttribute(handle);
            if (a.Constructor.Kind != HandleKind.MemberReference) continue;
            var ctor = md.GetMemberReference((MemberReferenceHandle)a.Constructor);
            if (ctor.Parent.Kind != HandleKind.TypeReference) continue;
            var blob = md.GetBlobReader(a.Value);
            if (blob.ReadUInt16() != 1) continue;  // the prolog
            try { result[md.GetString(md.GetTypeReference((TypeReferenceHandle)ctor.Parent).Name)] = blob.ReadSerializedString(); }
            catch (BadImageFormatException) { }  // not a string argument
        }
        return result;
    }
}
