using System;
using System.IO;
using System.Runtime.CompilerServices;
using System.Runtime.InteropServices;
using ExtendedResearch.Interop;

namespace ExtendedResearch.Interop.Tests;

/// <summary>crates/abi-testlib, declared the way a package declares its library.</summary>
internal static class Native
{
    public const string Library = "extendedresearch_abi_testlib";

    [DllImport(Library)]
    public static extern uint testlib_abi_version();

    [DllImport(Library)]
    public static extern ulong testlib_destroyed_count();

    [DllImport(Library)]
    public static extern int testlib_counter_create(byte[]? name, uint limit, out CounterHandle counter);

    [DllImport(Library)]
    public static extern void testlib_counter_destroy(IntPtr counter);

    [DllImport(Library)]
    public static extern int testlib_counter_name(CounterHandle counter, [Out] byte[]? destination, ulong capacity, out ulong length);

    [DllImport(Library)]
    public static extern int testlib_counter_name_bytes(CounterHandle counter, [Out] byte[]? destination, ulong capacity, out ulong length);

    [DllImport(Library)]
    public static extern int testlib_counter_increment(CounterHandle counter, out uint value);

    [DllImport(Library)]
    public static extern int testlib_counter_close(CounterHandle counter);

    [DllImport(Library)]
    public static extern int testlib_panic();

    [DllImport(Library)]
    public static extern int testlib_color_count(out uint count);

    [DllImport(Library)]
    public static extern int testlib_color_at(uint index, out int value);

    [DllImport(Library)]
    public static extern int testlib_color_name(int value, [Out] byte[]? destination, ulong capacity, out ulong length);
}

internal sealed class CounterHandle : AbiHandle
{
    protected override void Destroy(IntPtr handle) => Native.testlib_counter_destroy(handle);
}

internal static class Testlib
{
    public const int ErrEmptyName = AbiCodes.DomainFloor;
    public const int ErrFull = AbiCodes.DomainFloor - 1;

    public static readonly AbiErrors Errors = new(
        "TESTLIB",
        new[]
        {
            new System.Collections.Generic.KeyValuePair<int, string>(ErrEmptyName, "TESTLIB_ERR_EMPTY_NAME"),
            new System.Collections.Generic.KeyValuePair<int, string>(ErrFull, "TESTLIB_ERR_FULL"),
        });

    /// <summary>Whether the module initializer registered the resolver.</summary>
    public static bool Registered { get; private set; }

    [ModuleInitializer]
    internal static void Register()
    {
        Registered = AbiLibrary.TryRegisterResolver(typeof(Testlib).Assembly, Native.Library, Locate);
    }

    /// <summary>The built library: EXTENDEDRESEARCH_TESTLIB, or the first target/debug above this assembly.</summary>
    public static string? Locate()
    {
        var named = Environment.GetEnvironmentVariable("EXTENDEDRESEARCH_TESTLIB");
        if (!string.IsNullOrEmpty(named))
        {
            return named;
        }
        var file = OperatingSystem.IsWindows() ? Native.Library + ".dll"
            : OperatingSystem.IsMacOS() ? "lib" + Native.Library + ".dylib"
            : "lib" + Native.Library + ".so";
        for (var dir = new DirectoryInfo(AppContext.BaseDirectory); dir != null; dir = dir.Parent)
        {
            var candidate = Path.Combine(dir.FullName, "target", "debug", file);
            if (File.Exists(candidate))
            {
                return candidate;
            }
        }
        return null;
    }

    public static CounterHandle Create(string name, uint limit)
    {
        Errors.Check(
            Native.testlib_counter_create(AbiBuffer.ToUtf8Z(name, "name"), limit, out var counter),
            "testlib_counter_create");
        return counter;
    }
}
