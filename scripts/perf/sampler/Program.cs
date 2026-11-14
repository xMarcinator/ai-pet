// The sampler the Windows bench scripts read processes through (fnt-76 task 1), since Git Bash has no /proc for
// Windows programs. One line per call, numbers only, for the scripts to compute with:
//
//   sampler proc <pid>...   per pid: "<pid> <cpu ms> <working set kB> <private kB>", or "<pid> gone"
//   sampler system          "<busy ms> <elapsed ms>": all CPUs' busy time since boot, and the same in total, so two
//                           calls give the machine's load in cores (Windows has no load average)
//   sampler hold-port <port>  listens on 127.0.0.1:<port> until killed, so a pet started after it can't (its MCP
//                           listener off, for the paired runs)
//
// cpu ms is the process's user + kernel time; working set its resident memory (VmRSS's counterpart); private its
// private bytes (commit). Windows pids, as /proc/<msys pid>/winpid gives them in Git Bash. It runs on Linux too
// (from /proc), so the scripts' arithmetic can be tried there.
using System.Diagnostics;
using System.Globalization;
using System.Runtime.InteropServices;

var c = CultureInfo.InvariantCulture;
if (args.Length >= 2 && args[0] == "proc")
{
    foreach (var arg in args.Skip(1))
    {
        if (!int.TryParse(arg, out var pid)) { Console.Error.WriteLine($"not a pid: {arg}"); return 2; }
        try
        {
            using var p = Process.GetProcessById(pid);
            p.Refresh();
            var cpu = (long)p.TotalProcessorTime.TotalMilliseconds;
            Console.WriteLine(string.Create(c, $"{pid} {cpu} {p.WorkingSet64 / 1024} {p.PrivateMemorySize64 / 1024}"));
        }
        catch (Exception e) when (e is ArgumentException or InvalidOperationException)
        {
            Console.WriteLine($"{pid} gone");
        }
    }
    return 0;
}
if (args.Length == 2 && args[0] == "hold-port" && int.TryParse(args[1], out var port))
{
    var listener = new System.Net.Sockets.TcpListener(System.Net.IPAddress.Loopback, port);
    listener.Start();
    Console.WriteLine($"holding {port}");
    Thread.Sleep(Timeout.Infinite);
    return 0;
}
if (args.Length == 1 && args[0] == "system")
{
    var (busy, total) = SystemTimes();
    Console.WriteLine(string.Create(c, $"{busy} {total}"));
    return 0;
}
Console.Error.WriteLine("usage: sampler proc <pid>... | sampler system | sampler hold-port <port>");
return 2;

// Busy and total CPU time over all CPUs since boot, in ms.
static (long Busy, long Total) SystemTimes()
{
    if (OperatingSystem.IsWindows())
    {
        if (!GetSystemTimes(out var idle, out var kernel, out var user)) throw new InvalidOperationException("GetSystemTimes failed");
        // kernel time includes idle time; FILETIMEs are 100 ns
        long Ms(FileTime t) => (long)(((ulong)t.High << 32 | t.Low) / 10_000);
        var total = Ms(kernel) + Ms(user);
        return (total - Ms(idle), total);
    }
    // Linux: the cpu line of /proc/stat, in USER_HZ ticks (100 a second)
    var f = File.ReadLines("/proc/stat").First().Split(' ', StringSplitOptions.RemoveEmptyEntries).Skip(1)
        .Select(long.Parse).ToArray();
    var all = f.Take(8).Sum();
    var idleTicks = f[3] + f[4];
    return ((all - idleTicks) * 10, all * 10);
}

[DllImport("kernel32.dll", SetLastError = true)]
static extern bool GetSystemTimes(out FileTime idle, out FileTime kernel, out FileTime user);

[StructLayout(LayoutKind.Sequential)]
struct FileTime { public uint Low; public uint High; }
