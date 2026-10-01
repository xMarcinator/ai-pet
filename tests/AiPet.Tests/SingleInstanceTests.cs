using System.Diagnostics;
using System.IO;
using System.Reflection;
using System.Runtime.CompilerServices;
using System.Runtime.InteropServices;
using Xunit;

// A namespace block, not a file-scoped namespace: StartupHook, at the end, must be in no namespace at all.
namespace AiPet.Tests
{
    /// The Linux single-instance lock (Program.TakeLock), which a .NET pet and a Rust pet both take before anything
    /// else: an exclusive, non-blocking flock on the socket's path with .lock in place of .sock, or <AIPET_PIPE>.lock
    /// with that override. Each pet here is a process of its own that runs TakeLock from the app's AiPet.dll and
    /// nothing else of Main (see StartupHook): no window, no mutex. Each gets an AIPET_PIPE and a data folder of the
    /// test's, so none of them meets the user's pet or its lock. The pets that leave AIPET_PIPE unset only work out the
    /// lock's path, or run in a user and mount namespace of their own whose /run/user is a folder of the test's.
    public class SingleInstanceTests
    {
        /// Set for the pets this starts: the app's AiPet.dll, whose TakeLock they run.
        internal const string AppVar = "AIPET_TEST_LOCK_APP";
        const string RoleVar = "AIPET_TEST_LOCK_ROLE", Given = "AIPET_TEST_LOCK_GIVEN_";

        /// What a pet gets as the test gives them. TestEnv's module initializer sets its own in every process this
        /// assembly runs in, the pets' too, so they come under another name, and the pet puts them back.
        static readonly string[] PetVars = { "AIPET_PIPE", "AIPET_DATA_DIR" };

        /// Two pets started at the same moment on one lock, the AIPET_PIPE override's: one runs, and the other quits
        /// quietly. XDG_RUNTIME_DIR, different folders or none (snap, SSH), doesn't move an overridden lock. Pets on the
        /// default lock, with AIPET_PIPE unset, are PetsStartedTogether_OnTheDefaultLock_OneRuns.
        [LinuxTheory]
        [InlineData("a", "b")]
        [InlineData("a", null)]
        [InlineData(null, null)]
        public void PetsStartedTogether_OneRuns(string xdg1, string xdg2)
        {
            var pipe = Pipe();
            using var one = new Pet("lock", pipe, Xdg(xdg1));
            using var two = new Pet("lock", pipe, Xdg(xdg2));
            one.Ready();
            two.Ready();
            one.Go();
            two.Go();
            var said = new[] { one.Line(), two.Line() };
            Assert.Equal(new[] { "quit", "run" }, said.Order());
            var (runs, quits) = said[0] == "run" ? (one, two) : (two, one);
            Assert.Equal(0, quits.Exit());
            Assert.Equal("", quits.Stderr());

            // the pet that runs holds the lock, in a file only its user may open
            Assert.Null(ContractLock.Take(pipe + ".lock"));
            Assert.Equal(UnixFileMode.UserRead | UnixFileMode.UserWrite, File.GetUnixFileMode(pipe + ".lock"));
            Assert.Equal(0, runs.Stop());
        }

        /// A lock taken the contract's way (ContractLock, as the Rust pet takes it) keeps the .NET pet out, and the
        /// .NET pet's lock keeps it out, until the pet ends.
        [LinuxFact]
        public void ALockTakenTheContractsWay_AndThePetsLock_KeepEachOtherOut()
        {
            var pipe = Pipe();
            using (var held = ContractLock.Take(pipe + ".lock"))
            {
                Assert.NotNull(held);
                using var pet = new Pet("lock", pipe, null);
                Assert.Equal("quit", pet.Take());
                Assert.Equal(0, pet.Exit());
            }

            using var runs = new Pet("lock", pipe, null);
            Assert.Equal("run", runs.Take());
            Assert.Null(ContractLock.Take(pipe + ".lock"));
            Assert.Equal(0, runs.Stop());
            using var after = ContractLock.Take(pipe + ".lock");
            Assert.NotNull(after);
        }

        /// A pet that crashed holding the lock doesn't keep the next one out: the kernel let its lock go.
        [LinuxFact]
        public void ACrashedPetsLock_DoesntKeepTheNextOneOut()
        {
            var pipe = Pipe();
            using (var crashes = new Pet("lock", pipe, null))
            {
                Assert.Equal("run", crashes.Take());
                crashes.Kill();
            }
            using var next = new Pet("lock", pipe, null);
            Assert.Equal("run", next.Take());
            Assert.Equal(0, next.Stop());
        }

        /// A lock file that can't be opened, a symlink at its path (not followed, so the file it points to isn't made) or
        /// a folder, keeps every pet out: two pets started together both quit quietly rather than run without the lock,
        /// and each says why in its log.
        [LinuxTheory]
        [InlineData("symlink")]
        [InlineData("folder")]
        public void ALockFileThatCantBeOpened_KeepsEveryPetOut(string what)
        {
            var pipe = Pipe();
            var target = Path.Combine(TestEnv.NewDir("elsewhere"), "made");
            if (what == "symlink") File.CreateSymbolicLink(pipe + ".lock", target);
            else Directory.CreateDirectory(pipe + ".lock");
            using var one = new Pet("lock", pipe, null);
            using var two = new Pet("lock", pipe, null);
            one.Ready();
            two.Ready();
            one.Go();
            two.Go();
            Assert.Equal(new[] { "quit", "quit" }, new[] { one.Line(), two.Line() });
            foreach (var pet in new[] { one, two })
            {
                Assert.Equal(0, pet.Exit());
                Assert.Contains($"single instance: can't open {pipe}.lock", File.ReadAllText(Path.Combine(pet.DataDir, "aipet.log")));
            }
            Assert.False(File.Exists(target));
        }

        /// Two pets started at the same moment with AIPET_PIPE unset, so on the default lock, each with its own
        /// XDG_RUNTIME_DIR or none: both work out the lock in /run/user/<uid>, which the socket's rule takes before
        /// XDG_RUNTIME_DIR, and one runs. They run in a user and mount namespace of their own (as uid 0 there) whose
        /// /run/user is a folder of the test's, so they never meet the user's pet or its lock.
        [UserNamespaceTheory]
        [InlineData("a", "b")]
        [InlineData("a", null)]
        [InlineData(null, null)]
        public void PetsStartedTogether_OnTheDefaultLock_OneRuns(string xdg1, string xdg2)
        {
            var runUser = TestEnv.NewDir("run-user");
            Directory.CreateDirectory(Path.Combine(runUser, "0"), UnixFileMode.UserRead | UnixFileMode.UserWrite | UnixFileMode.UserExecute);
            foreach (var xdg in new[] { xdg1, xdg2 })
            {
                using var where = new Pet("path", null, Xdg(xdg), runUser: runUser);
                Assert.Equal(("/run/user/0/aipet.sock", "/run/user/0/aipet.lock"), where.Paths());
            }

            using var one = new Pet("lock", null, Xdg(xdg1), runUser: runUser);
            using var two = new Pet("lock", null, Xdg(xdg2), runUser: runUser);
            one.Ready();
            two.Ready();
            one.Go();
            two.Go();
            var said = new[] { one.Line(), two.Line() };
            Assert.Equal(new[] { "quit", "run" }, said.Order());
            var (runs, quits) = said[0] == "run" ? (one, two) : (two, one);
            Assert.Equal(0, quits.Exit());

            // the pet that runs holds the lock in the test's /run/user/0, in a file only its user may open
            var lockFile = Path.Combine(runUser, "0", "aipet.lock");
            Assert.Null(ContractLock.Take(lockFile));
            Assert.Equal(UnixFileMode.UserRead | UnixFileMode.UserWrite, File.GetUnixFileMode(lockFile));
            Assert.Equal(0, runs.Stop());
        }

        /// Where the lock is, worked out by pets that don't take it: next to the socket, the socket's path with .lock
        /// in place of .sock, whatever XDG_RUNTIME_DIR says, and one file for them all where the user has a /run/user
        /// folder (logind's, which the socket's rule takes first). With the AIPET_PIPE override it is
        /// <AIPET_PIPE>.lock: .lock after the whole path, .sock and all, a relative one taken from the working folder
        /// as the socket's is.
        [LinuxFact]
        public void TheLock_IsNextToTheSocket()
        {
            var paths = new[] { Xdg("a"), Xdg("b"), null }.Select(xdg =>
            {
                using var pet = new Pet("path", null, xdg);
                return pet.Paths();
            }).ToList();
            foreach (var (socket, lockFile) in paths)
            {
                Assert.EndsWith("/aipet.sock", socket);
                Assert.Equal(socket[..^".sock".Length] + ".lock", lockFile);
            }
            var login = "/run/user/" + geteuid();
            if (Directory.Exists(login)) Assert.All(paths, p => Assert.Equal(login + "/aipet.lock", p.Lock));

            var pipe = Pipe();
            using (var given = new Pet("path", pipe, null)) Assert.Equal((pipe, pipe + ".lock"), given.Paths());
            var cwd = TestEnv.NewDir("cwd");
            using var relative = new Pet("path", "pets/pet.sock", null, cwd);
            var socketPath = Path.Combine(cwd, "pets", "pet.sock");
            Assert.Equal((socketPath, socketPath + ".lock"), relative.Paths());
        }

        /// A socket path in a folder of the test's own, so the lock file next to it goes with the test's files.
        static string Pipe() => Path.Combine(TestEnv.NewDir("lock"), "aipet.sock");

        static string Xdg(string name) => name == null ? null : TestEnv.NewDir("xdg-" + name);

        /// A pet: `dotnet AiPet.Tests.dll`, whose Main does nothing, with this assembly as its startup hook, which
        /// runs RunPet. Its own data folder, and the AIPET_PIPE and XDG_RUNTIME_DIR given (null: unset). With runUser,
        /// in a user and mount namespace of its own (UserNamespace) whose /run/user is that folder.
        sealed class Pet : IDisposable
        {
            readonly Process process;
            readonly Task<string> stderr;
            public readonly string DataDir = TestEnv.NewDir("pet-data");

            public Pet(string role, string pipe, string xdg, string cwd = null, string runUser = null)
            {
                var tests = typeof(SingleInstanceTests).Assembly.Location;
                var psi = new ProcessStartInfo(runUser == null ? TestEnv.Dotnet : "unshare")
                {
                    UseShellExecute = false, RedirectStandardInput = true, RedirectStandardOutput = true,
                    RedirectStandardError = true, WorkingDirectory = cwd ?? DataDir,
                };
                if (runUser != null)
                {
                    foreach (var arg in UserNamespace.Args(runUser)) psi.ArgumentList.Add(arg);
                    psi.ArgumentList.Add(TestEnv.Dotnet);
                }
                psi.ArgumentList.Add(tests);
                psi.Environment["DOTNET_STARTUP_HOOKS"] = tests;
                psi.Environment[AppVar] = ResourceTests.AppDll;
                psi.Environment[RoleVar] = role;
                psi.Environment[Given + "AIPET_PIPE"] = pipe ?? "";
                psi.Environment[Given + "AIPET_DATA_DIR"] = DataDir;
                if (xdg == null) psi.Environment.Remove("XDG_RUNTIME_DIR");
                else psi.Environment["XDG_RUNTIME_DIR"] = xdg;
                process = Process.Start(psi);
                stderr = process.StandardError.ReadToEndAsync();
            }

            public void Ready() => Assert.Equal("ready", Line());

            public void Go()
            {
                process.StandardInput.WriteLine("go");
                process.StandardInput.Flush();
            }

            /// Takes the lock as Main does: run, or quit.
            public string Take()
            {
                Ready();
                Go();
                return Line();
            }

            /// What a pet of role path worked out: the socket's path, then the lock's.
            public (string Socket, string Lock) Paths() => (Line(), Line());

            /// Its next line on stdout.
            public string Line()
            {
                var line = process.StandardOutput.ReadLineAsync();
                Assert.True(line.Wait(30000), "the pet said nothing within 30 s");
                if (line.Result == null) Assert.Fail($"the pet ended (exit code {Exit()}): {Stderr()}");
                return line.Result;
            }

            /// Ends a pet that runs, as its quitting would: it ends when its stdin closes.
            public int Stop()
            {
                process.StandardInput.Close();
                return Exit();
            }

            public int Exit()
            {
                Assert.True(process.WaitForExit(30000), "the pet didn't end within 30 s");
                return process.ExitCode;
            }

            public string Stderr()
            {
                Exit();
                Assert.True(stderr.Wait(30000), "the pet's stderr didn't close");
                return stderr.Result;
            }

            /// A crash: SIGKILL.
            public void Kill()
            {
                process.Kill();
                Exit();
            }

            public void Dispose()
            {
                try { if (!process.HasExited) process.Kill(); }
                catch (Exception) { }
                process.Dispose();
            }
        }

        /// In a pet's process (StartupHook): puts back the variables the test gave it, then by its role either works
        /// out the paths (path: the socket's, then the lock's), or takes the lock (lock): it says ready, waits for go,
        /// takes the lock as Main does, and says run or quit. A pet that runs keeps the lock until its stdin closes.
        internal static int RunPet(string app)
        {
            foreach (var name in PetVars) Environment.SetEnvironmentVariable(name, Environment.GetEnvironmentVariable(Given + name));
            const BindingFlags Private = BindingFlags.NonPublic | BindingFlags.Static;
            var program = Assembly.LoadFrom(app).GetType("AiPet.Program", throwOnError: true);
            var lockFile = program.GetProperty("LockFile", Private) ?? throw new MissingMemberException("AiPet.Program", "LockFile");
            var take = program.GetMethod("TakeLock", Private) ?? throw new MissingMethodException("AiPet.Program", "TakeLock");
            if (Environment.GetEnvironmentVariable(RoleVar) == "path")
            {
                Console.WriteLine(Ipc.Endpoint);
                Console.WriteLine(lockFile.GetValue(null));
                return 0;
            }

            // everything but the open and the flock done before go, so the pets take the lock at the same moment
            lockFile.GetValue(null);
            RuntimeHelpers.PrepareMethod(take.MethodHandle);
            Console.WriteLine("ready");
            if (Console.ReadLine() != "go") return 3;
            bool runs = (bool)take.Invoke(null, null);
            Console.WriteLine(runs ? "run" : "quit");
            if (runs) Console.ReadLine();
            return 0;
        }

        /// A lock taken the contract's way, as the Rust pet takes it, and not with the app's code: the file opened
        /// O_RDWR | O_CREAT | O_NOFOLLOW | O_CLOEXEC with mode 0600, then flock(LOCK_EX | LOCK_NB). Null while another
        /// holds it. O_CLOEXEC keeps it out of the pets this process starts meanwhile.
        sealed class ContractLock : IDisposable
        {
            readonly int fd;

            ContractLock(int fd) => this.fd = fd;

            public static ContractLock Take(string path)
            {
                int nofollow = RuntimeInformation.ProcessArchitecture is Architecture.Arm or Architecture.Arm64
                    or Architecture.Ppc64le ? 0x8000 : 0x20000;
                int fd = open(path, 0x2 | 0x40 | 0x80000 | nofollow, 0x180);
                Assert.True(fd >= 0, $"open {path}: {Marshal.GetPInvokeErrorMessage(Marshal.GetLastPInvokeError())}");
                if (flock(fd, 2 | 4) == 0) return new ContractLock(fd);
                int error = Marshal.GetLastPInvokeError();
                close(fd);
                // EWOULDBLOCK: another holds it
                Assert.True(error == 11, $"flock {path}: {Marshal.GetPInvokeErrorMessage(error)}");
                return null;
            }

            public void Dispose() => close(fd);
        }

        /// A user and mount namespace of the pet's own, made by util-linux's unshare with uid 0 in it, where sh binds a
        /// folder of the test's over /run/user before it runs the pet. The bind stays in that namespace: the pet's
        /// /run/user/0 is the folder's 0, and nothing outside it changes.
        static class UserNamespace
        {
            public static string[] Args(string runUser) =>
                new[] { "--user", "--map-root-user", "--mount", "sh", "-c", "mount --bind \"$0\" /run/user && exec \"$@\"", runUser };

            /// Null where a pet can run that way, else why not (no unshare, no /run/user, or no unprivileged user
            /// namespaces).
            public static string Unavailable => unavailable.Value;

            static readonly Lazy<string> unavailable = new(() =>
            {
                // the pet's own files must stay in sight in the namespace
                if (new[] { Path.GetTempPath(), AppContext.BaseDirectory, TestEnv.Dotnet }.Any(p => p.StartsWith("/run/user/", StringComparison.Ordinal)))
                    return "the tests' files are under /run/user, which the namespace covers";
                try
                {
                    var psi = new ProcessStartInfo("unshare")
                    {
                        UseShellExecute = false, RedirectStandardOutput = true, RedirectStandardError = true,
                    };
                    foreach (var arg in Args(Path.GetTempPath())) psi.ArgumentList.Add(arg);
                    psi.ArgumentList.Add("true");
                    using var p = Process.Start(psi);
                    _ = p.StandardOutput.ReadToEndAsync();
                    var stderr = p.StandardError.ReadToEndAsync();
                    if (!p.WaitForExit(10000))
                    {
                        p.Kill();
                        return "unshare didn't end within 10 s";
                    }
                    return p.ExitCode == 0 ? null : $"no user and mount namespace with /run/user bound here: {stderr.Result.Trim()}";
                }
                catch (Exception ex) { return "no unshare: " + ex.Message; }
            });
        }

        /// Linux, where a pet can run in a namespace of its own (UserNamespace); skipped elsewhere.
        sealed class UserNamespaceTheoryAttribute : TheoryAttribute
        {
            public UserNamespaceTheoryAttribute()
            {
                if (!OperatingSystem.IsLinux()) Skip = "Linux only";
                else if (UserNamespace.Unavailable is { } why) Skip = why;
            }
        }

        [DllImport("libc", SetLastError = true)]
        static extern int open([MarshalAs(UnmanagedType.LPUTF8Str)] string path, int flags, uint mode);
        [DllImport("libc", SetLastError = true)] static extern int flock(int fd, int operation);
        [DllImport("libc")] static extern int close(int fd);
        [DllImport("libc")] static extern uint geteuid();
    }
}

/// The pets SingleInstanceTests starts name this assembly in DOTNET_STARTUP_HOOKS, so this runs before their Main (an
/// empty one) and ends them. Any other process this assembly runs in goes straight on.
static class StartupHook
{
    public static void Initialize()
    {
        if (Environment.GetEnvironmentVariable(AiPet.Tests.SingleInstanceTests.AppVar) is not { Length: > 0 } app) return;
        int code;
        try { code = AiPet.Tests.SingleInstanceTests.RunPet(app); }
        catch (Exception ex)
        {
            Console.Error.WriteLine(ex);
            code = 2;
        }
        Environment.Exit(code);
    }
}
