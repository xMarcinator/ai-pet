using System.IO;
using Xunit;

namespace AiPet.Tests;

/// release.yml's scripts, run as the runner runs them (bash -eo pipefail), against folders and git repositories the
/// tests make. gh is a stand-in; nothing reaches GitHub.
public class ReleaseWorkflowTests
{
    const string Workflow = "release.yml";
    const string Sha = "0123456789abcdef0123456789abcdef01234567";
    readonly Scripts.Sandbox box = new();

    Dictionary<string, string> Outputs(string file) => File.Exists(file)
        ? File.ReadAllLines(file).Where(l => l.Contains('=')).ToDictionary(l => l[..l.IndexOf('=')], l => l[(l.IndexOf('=') + 1)..])
        : new Dictionary<string, string>();

    /// Only a stable release that isn't older than the one main pins moves main's marketplaces (every user's plugin)
    /// and becomes GitHub's latest release (the one-line installers' app).
    [UnixTheory]
    [InlineData("0.3.0", "v0.2.0", Sha, true)]
    [InlineData("0.3.0-rc.1", "v0.2.0", Sha, false)]
    [InlineData("0.1.9", "v0.2.0", Sha, false)]              // an older line: main stays on the newer one
    [InlineData("0.10.0", "v0.9.0", Sha, true)]              // by version, not by text
    [InlineData("0.2.0", "v0.2.0", Sha, true)]               // a re-run
    [InlineData("0.3.0", "v0.3.0-rc.1", Sha, true)]          // an older workflow pinned a prerelease
    [InlineData("0.1.0", "v0.2.0", "0000000000000000000000000000000000000000", true)]  // the placeholder: nothing released yet
    public void LatestCheck_ByVersionAndPin(string version, string pinnedRef, string pinnedSha, bool latest)
    {
        // The step reads only the first plugin's source ref and sha. Not the repository's own files: every release
        // pins them anew.
        var cwd = box.Dir("repo");
        foreach (var f in new[] { ".claude-plugin/marketplace.json", ".agents/plugins/marketplace.json" })
        {
            Directory.CreateDirectory(Path.GetDirectoryName(Path.Combine(cwd, f)));
            File.WriteAllText(Path.Combine(cwd, f), $$"""
                { "plugins": [ { "name": "aipet", "source": { "source": "git-subdir", "ref": "{{pinnedRef}}", "sha": "{{pinnedSha}}" } } ] }
                """);
        }
        var output = Path.Combine(box.Root, "output");
        box.Env["VERSION"] = version;
        box.Env["GITHUB_OUTPUT"] = output;
        box.Env["GITHUB_STEP_SUMMARY"] = Path.Combine(box.Root, "summary");

        var r = box.Step(Scripts.WorkflowStep(Workflow, "Check whether this release is the latest"), cwd);
        Assert.True(r.Exit == 0, r.Output);
        Assert.Equal(latest ? "true" : "false", Outputs(output).GetValueOrDefault("latest"));
    }

    [UnixFact]
    public void Pin_RunsOnlyForTheLatestRelease()
    {
        var step = Scripts.WorkflowStepText(Workflow, "Pin the marketplaces to the plugin commit");
        Assert.Contains("if: steps.latest.outputs.latest == 'true'", step);
        Assert.Contains("--latest=\"$LATEST\"", Scripts.WorkflowStep(Workflow, "Create the GitHub release"));
    }

    /// A repository whose codex.json is A at v0.2.0, B at v0.3.0-rc.1, and B at HEAD (one commit later).
    string HooksChangedInAPrerelease()
    {
        var repo = box.Dir("repo");
        box.Git(repo, "init", "-q");
        var codex = Path.Combine(repo, "plugins", "aipet", "hooks", "codex.json");
        Directory.CreateDirectory(Path.GetDirectoryName(codex));
        File.WriteAllText(codex, "{\"a\":1}\n");
        box.Git(repo, "add", "-A");
        box.Git(repo, "commit", "-q", "-m", "0.2.0");
        box.Git(repo, "tag", "v0.2.0");
        File.WriteAllText(codex, "{\"b\":2}\n");
        box.Git(repo, "commit", "-q", "-am", "hooks change");
        box.Git(repo, "tag", "v0.3.0-rc.1");
        File.WriteAllText(Path.Combine(repo, "README.md"), "more\n");
        box.Git(repo, "add", "-A");
        box.Git(repo, "commit", "-q", "-m", "more");
        return repo;
    }

    (string Notes, string Create) Release(string repo, string version, string latest = "true")
    {
        Directory.CreateDirectory(Path.Combine(repo, "dist"));
        File.WriteAllText(Path.Combine(repo, "dist", "SHA256SUMS"), "x\n");
        box.Stub("gh", """
            case "$1 $2" in
              "release view") exit 1 ;;
              "release create")
                echo "$*" > "$AIPET_TEST_OUT/gh-create"
                while [ $# -gt 0 ]; do
                  if [ "$1" = --notes-file ]; then cp "$2" "$AIPET_TEST_OUT/notes.md"; fi
                  shift
                done ;;
              *) echo "unexpected: gh $*" >&2; exit 1 ;;
            esac
            """);
        foreach (var (k, v) in new Dictionary<string, string>
        {
            ["VERSION"] = version, ["GITHUB_SHA"] = box.Git(repo, "rev-parse", "HEAD"), ["GITHUB_REPOSITORY"] = "xMarcinator/ai-pet",
            ["PLUGIN_REPO"] = "xMarcinator/ai-pet-plugin", ["PLUGIN_PATH"] = "plugins/aipet", ["PLUGIN_SHA"] = Sha, ["LATEST"] = latest,
        }) box.Env[k] = v;
        var r = box.Step(Scripts.WorkflowStep(Workflow, "Create the GitHub release"), repo);
        Assert.True(r.Exit == 0, r.Output);
        return (File.ReadAllText(Path.Combine(box.Out, "notes.md")), File.ReadAllText(Path.Combine(box.Out, "gh-create")));
    }

    /// Stable users update from the latest stable release (prereleases don't move the marketplaces), so a stable
    /// release after a prerelease that changed the Codex hooks still warns that Codex asks to trust them again.
    [UnixFact]
    public void Notes_StableAfterAPrerelease_ComparesWithTheLatestStableRelease()
    {
        var (notes, create) = Release(HooksChangedInAPrerelease(), "0.3.0");
        Assert.Contains("**Codex users:**", notes);
        Assert.Contains("--latest=true", create);
        Assert.DoesNotContain("--prerelease", create);
    }

    [UnixFact]
    public void Notes_Prerelease_ComparesWithTheLatestStableRelease()
    {
        var (notes, create) = Release(HooksChangedInAPrerelease(), "0.3.0-rc.2", "false");
        Assert.Contains("**Codex users:**", notes);
        Assert.Contains("--prerelease", create);
        Assert.Contains("--latest=false", create);
    }

    [UnixFact]
    public void Notes_UnchangedHooks_NoWarning()
    {
        var repo = box.Dir("repo");
        box.Git(repo, "init", "-q");
        var codex = Path.Combine(repo, "plugins", "aipet", "hooks", "codex.json");
        Directory.CreateDirectory(Path.GetDirectoryName(codex));
        File.WriteAllText(codex, "{\"a\":1}\n");
        box.Git(repo, "add", "-A");
        box.Git(repo, "commit", "-q", "-m", "0.2.0");
        box.Git(repo, "tag", "v0.2.0");
        File.WriteAllText(Path.Combine(repo, "README.md"), "more\n");
        box.Git(repo, "add", "-A");
        box.Git(repo, "commit", "-q", "-m", "more");

        Assert.DoesNotContain("Codex users", Release(repo, "0.2.1").Notes);
    }

    /// actions/checkout can't check out a repository without commits, which the plugin job would find only after
    /// the builds: the check job fails at once. It reads the repository over ssh with the deploy key, as the plugin
    /// job does, so a private repository is checked too, and a missing or wrong key fails as early.
    [UnixTheory]
    [InlineData("empty", "has no branch")]
    [InlineData("full", null)]
    [InlineData("missing", "Couldn't read")]
    [InlineData("nokey", "PLUGIN_DEPLOY_KEY isn't set")]
    public void Check_PluginRepositoryWithoutABranch_FailsEarly(string repo, string error)
    {
        var server = box.Dir("server");
        box.Git(server, "init", "-q", "--bare", "owner/empty.git");
        box.Git(server, "init", "-q", "--bare", "owner/full.git");
        var work = box.Dir("work");
        box.Git(work, "init", "-q");
        File.WriteAllText(Path.Combine(work, "README.md"), "plugin\n");
        box.Git(work, "add", "-A");
        box.Git(work, "commit", "-q", "-m", "first");
        box.Git(work, "push", "-q", Path.Combine(server, "owner", "full.git"), "HEAD:refs/heads/main");
        // github.com's ssh is this folder: the stand-in writes down how it was run and runs the remote command here
        box.Stub("ssh", $$"""
            echo "$*" > "$AIPET_TEST_OUT/ssh-args"
            key=
            while [ $# -gt 1 ]; do
              if [ "$1" = -i ]; then key=$2; fi
              shift
            done
            ls -l "$key" | cut -c1-10 > "$AIPET_TEST_OUT/ssh-key-mode"
            cp "$key" "$AIPET_TEST_OUT/ssh-key"
            cd '{{server}}' && eval "git ${1#git-}"
            """);
        var temp = box.Dir("runner-temp");
        box.Env["RUNNER_TEMP"] = temp;
        box.Env["PLUGIN_REPO"] = "owner/" + (repo == "nokey" ? "full" : repo);
        box.Env["DEPLOY_KEY"] = repo == "nokey" ? "" : "-----BEGIN OPENSSH PRIVATE KEY-----\ntest\n-----END OPENSSH PRIVATE KEY-----";
        box.Env["GITHUB_HOST_KEY"] = "ssh-ed25519 AAAA";

        Assert.Contains("DEPLOY_KEY: ${{ secrets.PLUGIN_DEPLOY_KEY }}", Scripts.WorkflowStepText(Workflow, "Check the plugin repository has a branch"));
        var r = box.Step(Scripts.WorkflowStep(Workflow, "Check the plugin repository has a branch"), box.Root);
        Assert.True(r.Exit == (error == null ? 0 : 1), r.Output);
        if (error == null) Assert.DoesNotContain("::", r.Output);
        else
        {
            Assert.Contains("::error::", r.Output);
            Assert.Contains(error, r.Output);
        }
        Assert.False(File.Exists(Path.Combine(temp, "plugin-deploy-key")), "the key stays on disk");
        if (repo == "nokey")
        {
            Assert.False(File.Exists(Path.Combine(box.Out, "ssh-args")));
            return;
        }
        var args = File.ReadAllText(Path.Combine(box.Out, "ssh-args"));
        Assert.Contains("git@github.com", args);
        Assert.Contains("IdentitiesOnly=yes", args);
        Assert.Contains("StrictHostKeyChecking=yes", args);
        Assert.Equal("-rw-------", File.ReadAllText(Path.Combine(box.Out, "ssh-key-mode")).Trim());
        Assert.Equal(box.Env["DEPLOY_KEY"] + "\n", File.ReadAllText(Path.Combine(box.Out, "ssh-key")));
        Assert.Equal("github.com ssh-ed25519 AAAA\n", File.ReadAllText(Path.Combine(temp, "plugin-known-hosts")));
    }
}
