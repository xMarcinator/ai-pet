using System.Reflection;
using System.Text.Json;

namespace AiPet.Golden;

/// Mode ipc: the endpoint, the data paths and the user's folders as the C# finds them in this process's environment,
/// as one JSON object. rust/crates/aipet-ipc (src/csharp.rs) runs it in a series of environments and computes the
/// same in each. Unlike the other modes it gets no temp AIPET_DATA_DIR, since the environment is what it reports; it
/// writes nothing.
static class IpcMode
{
    public static int Run(string[] args)
    {
        if (args.Length > 0) return Program.Usage();
        // internal to Core; the Unix endpoint is named after it (null without /proc: Windows, macOS)
        var effectiveUid = typeof(Ipc).GetMethod("EffectiveUid", BindingFlags.NonPublic | BindingFlags.Static)
                           ?? throw new InvalidOperationException("Ipc.EffectiveUid not found");
        Console.WriteLine(JsonSerializer.Serialize(new Dictionary<string, string>
        {
            ["endpoint"] = Ipc.Endpoint,
            ["data_dir"] = Paths.DataDir,
            ["config"] = Paths.Config,
            ["log"] = Paths.Log,
            ["hook_events_log"] = Paths.HookEventsLog,
            ["codex_home"] = Paths.CodexHome,
            ["home"] = Environment.GetFolderPath(Environment.SpecialFolder.UserProfile),
            ["local_app_data"] = Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData),
            ["effective_uid"] = (string)effectiveUid.Invoke(null, null),
        }));
        return 0;
    }
}
