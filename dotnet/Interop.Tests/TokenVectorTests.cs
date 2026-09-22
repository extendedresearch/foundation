// The shared-rule vector for the error-token grammar, run against the C# half
// of it.
//
// The grammar is written twice: `AbiErrors.Token` and `AbiErrors.NameOf` here,
// and `extendedresearch_status::codes::token` in Rust, which
// `crates/status/tests/vectors.rs` runs against this same file. Neither can
// call the other — this C# ships as source and is compiled into a consumer's
// own assembly, with no Rust anywhere near it — so the rule is registered as a
// `[[shared_rule]]` in `ecosystem/PACKAGES.toml`.
//
// A row is never edited to match an implementation. When one fails, either the
// implementation is wrong or the rule `docs/conventions/error-tokens.md` states
// changed.

using System;
using System.Collections.Generic;
using System.IO;
using System.Text.Json;
using ExtendedResearch.Interop;
using Xunit;

namespace ExtendedResearch.Interop.Tests;

public class TokenVectorTests
{
    private const string Vector =
        "0001-an-error-token-is-the-header-spelling-of-a-constant";

    /// <summary>The vector file, found by walking up from this assembly.</summary>
    private static string Locate()
    {
        var relative = Path.Combine("crates", "status", "vectors", Vector + ".json");
        for (var dir = new DirectoryInfo(AppContext.BaseDirectory); dir != null; dir = dir.Parent)
        {
            var candidate = Path.Combine(dir.FullName, relative);
            if (File.Exists(candidate))
            {
                return candidate;
            }
        }
        throw new FileNotFoundException("no " + relative + " above " + AppContext.BaseDirectory);
    }

    /// <summary>
    /// The token this side answers for one row. `name_of` builds the
    /// translation the row's registration describes and asks it, which is the
    /// join a package performs; `token` is the prefix step on its own.
    /// </summary>
    private static string Observe(JsonElement input)
    {
        var prefix = input.GetProperty("prefix").GetString()!;
        switch (input.GetProperty("call").GetString())
        {
            case "token":
                return AbiErrors.Token(prefix, input.GetProperty("name").GetString()!);
            case "name_of":
                var code = int.Parse(input.GetProperty("code").GetString()!);
                var domain = new List<KeyValuePair<int, string>>();
                foreach (var entry in input.GetProperty("domain").EnumerateObject())
                {
                    domain.Add(new KeyValuePair<int, string>(
                        int.Parse(entry.Name), entry.Value.GetString()!));
                }
                return new AbiErrors(prefix, domain).NameOf(code);
            default:
                throw new InvalidOperationException(
                    "no runner for call " + input.GetProperty("call").GetString());
        }
    }

    [Fact]
    public void EveryRowOfTheSharedVectorPasses()
    {
        using var file = JsonDocument.Parse(File.ReadAllText(Locate()));
        var vector = file.RootElement;
        Assert.Equal("0", vector.GetProperty("shared_rule_vector").GetString());
        Assert.Equal(Vector, vector.GetProperty("id").GetString());
        Assert.Equal("error_token", vector.GetProperty("function").GetString());
        Assert.NotEmpty(vector.GetProperty("about").GetString()!);

        var failures = new List<string>();
        var checked_ = 0;
        foreach (var row in vector.GetProperty("rows").EnumerateObject())
        {
            var want = row.Value.GetProperty("expect").GetString()!;
            var got = Observe(row.Value.GetProperty("input"));
            checked_ += 1;
            if (got != want)
            {
                failures.Add(row.Name + ": expected " + want + ", got " + got);
            }
        }
        Assert.Equal(new List<string>(), failures);
        // Guards against a runner that silently reads nothing.
        Assert.True(checked_ >= 20, "only " + checked_ + " rows checked");
    }
}
