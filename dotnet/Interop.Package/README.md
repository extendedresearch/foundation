# ExtendedResearch.Interop

C# source that a package's .NET binding compiles into its own assembly, for
`netstandard2.1` and `net8.0`.

## What it is

A .NET binding to a Rust library reaches it through P/Invoke, and the same
handful of problems arrive every time: freeing the handle exactly once and from
whatever thread the finalizer runs on, reading a variable-length answer out of
a buffer the caller allocated, turning an `int32_t` status into an exception a
caller can catch, and finding the native library at all. This package is that
code, written once, for a library that follows the conventions
`extendedresearch-abi` states.

| Type | What it does |
|---|---|
| `AbiHandle` | A `SafeHandle` whose release calls the library's `_destroy` |
| `AbiErrors`, `AbiException` | A status code to the package's exception |
| `AbiBuffer` | Measure-then-copy reads of text and bytes |
| `AbiLibrary` | A `DllImport` resolver, on net8.0 |
| `AbiEnumeration` | An enumeration's `_count`/`_at`/`_name` read into a list |
| `AbiCodes` | The boundary codes |

Every type is `internal`, in the namespace `ExtendedResearch.Interop`.

**The package holds source files and no assembly.** Each file is under
`contentFiles/cs/any/ExtendedResearch.Interop/` with the build action
`Compile`, so a `PackageReference` compiles it into your project. The package
is marked `developmentDependency`, which the
[nuspec reference](https://learn.microsoft.com/en-us/nuget/reference/nuspec#developmentdependency)
describes as preventing it from being included as a dependency in other
packages.

## Install

The package is attached to each foundation release and is not on nuget.org.
NuGet reads a folder of `.nupkg` files as a package source
([local feeds](https://learn.microsoft.com/en-us/nuget/hosting-packages/local-feeds)),
so download the package into a folder in your repository:

```bash
mkdir -p nuget
curl -fL -o nuget/ExtendedResearch.Interop.0.1.1.nupkg \
  https://github.com/extendedresearch/foundation/releases/download/v0.1.1/ExtendedResearch.Interop.0.1.1.nupkg
```

Name the folder in a `nuget.config` beside your project, and map the package id
to it so restore takes it from that folder alone
([package source mapping](https://learn.microsoft.com/en-us/nuget/consume-packages/package-source-mapping)):

```xml
<?xml version="1.0" encoding="utf-8"?>
<configuration>
  <packageSources>
    <clear />
    <add key="nuget.org" value="https://api.nuget.org/v3/index.json" />
    <add key="foundation" value="nuget" />
  </packageSources>
  <packageSourceMapping>
    <packageSource key="nuget.org">
      <package pattern="*" />
    </packageSource>
    <packageSource key="foundation">
      <package pattern="ExtendedResearch.Interop" />
    </packageSource>
  </packageSourceMapping>
</configuration>
```

Then reference it by exact version:

```xml
<ItemGroup>
  <PackageReference Include="ExtendedResearch.Interop" Version="[0.1.1]" PrivateAssets="all" />
</ItemGroup>
```

The release's `SHA256SUMS` covers the `.nupkg`.

## Use

Declare the library's exports, derive one handle class per handle type, and
build the error translation from your package's prefix and its domain codes:

```csharp
internal static class Native
{
    public const string Library = "example";

    [DllImport(Library)]
    public static extern void example_stream_destroy(IntPtr stream);

    [DllImport(Library)]
    public static extern int example_stream_name(
        StreamHandle stream, [Out] byte[]? destination, ulong capacity, out ulong length);
}

/// One handle type, one _destroy.
internal sealed class StreamHandle : AbiHandle
{
    protected override void Destroy(IntPtr handle) => Native.example_stream_destroy(handle);
}

internal sealed class Stream : IDisposable
{
    /// The package's prefix, and every domain code its header declares.
    private static readonly AbiErrors Errors = new AbiErrors(
        "EXAMPLE",
        new[] { new KeyValuePair<int, string>(-16, "EXAMPLE_ERR_TRUNCATED") });

    private readonly StreamHandle _handle;

    internal Stream(StreamHandle handle) => _handle = handle;

    /// Measure, allocate, copy — raising the package's exception on a refusal.
    public string Name => AbiBuffer.ReadText(
        (byte[]? destination, ulong capacity, out ulong length) =>
            Native.example_stream_name(_handle, destination, capacity, out length),
        Errors,
        "example_stream_name");

    public void Dispose() => _handle.Dispose();
}
```

Declare the destination as `[Out] byte[]? destination`: a null array is a null
pointer, which measures, and a `byte[]` is pinned for the call, so the library
writes straight into it. On net8.0, call
`AbiLibrary.TryRegisterResolver` once before the first P/Invoke to point
`[DllImport]` at a library the default search would not find.

`dotnet/Interop.PackageTest` in foundation is a consumer of exactly this shape:
it restores the package from a local folder and builds for `netstandard2.1` and
`net8.0` with C# 8 and warnings as errors.

## Guarantees

- **The library's `_destroy` runs exactly once, or not at all.**
  `AbiHandle.ReleaseHandle` is sealed and `SafeHandle` calls it once, and never
  for a handle that is `IsInvalid`. `Destroy` may run from `Dispose()` or from
  the finalizer thread in an order nothing controls, which is why every
  `_destroy` in this ecosystem accepts null, does not block, and may be called
  from any thread.
- **The codes here cannot drift from the Rust.** `AbiCodes.cs` is the .NET form
  of `extendedresearch_abi::codes`, written by hand, and
  `crates/abi-testlib/tests/interop_codes.rs` compares every value and every
  name against the Rust constants.
- **Names follow the header.** A boundary code is your prefix and the boundary
  name (`EXAMPLE_ERR_NULL`), a domain code is the name you declared
  (`EXAMPLE_ERR_TIMEOUT`), and any other code is `<PREFIX>_ERR_UNKNOWN`. The
  pyo3 and napi layers spell a failure the same way.
- **Where no hook answers, the mapping is fixed**: `ERR_UTF8` raises
  `ArgumentException` and `ERR_RANGE` raises `ArgumentOutOfRangeException`,
  each with an `AbiException` inside carrying the code and the name; `ERR_PANIC`
  raises `AbiPanicException`; anything else raises `AbiException`.
- **Every failure of the binding goes through one place.**
  `AbiErrors.BindingFailure` builds each one, so a package that passes the
  binding-failure hook never lets an internal type reach its callers.
- **`RequireVersion` compares for equality, not "at least".** The conventions
  are an ownership contract, not a feature set a later version is a superset
  of.
- **Text crossing the boundary is strict UTF-8 in both directions.** A decoder
  fallback on the way in is a binding failure rather than a replacement
  character; `ToUtf8Z` refuses a string holding a null character, which would
  end it early, or a lone surrogate, which has no UTF-8 form.
- **The sources are compiled, run and packed on every pull request.**
  `dotnet build dotnet/Interop.Build` compiles them for both targets at C# 8
  with warnings as errors; `dotnet test dotnet/Interop.Tests` P/Invokes a real
  Rust library with them on net8.0; and the `release-assets` job packs the
  `.nupkg`, checks what it holds, and builds `dotnet/Interop.PackageTest`
  against it through a local folder source.

## Limits

- **Every type is `internal` to the assembly that compiles it.** A caller
  outside that assembly can catch these exceptions only as `Exception`. A
  package whose callers should catch by type passes `AbiErrors` two hooks that
  answer its own public exception types: one for a failing code, one for a
  failure of the binding.
- **Two assemblies compiling this package have two unrelated sets of types.**
  The package carries no assembly and is a `developmentDependency`, so nothing
  is shared between them — an `AbiException` from one is not the other's, and
  `catch` does not cross.
- **`AbiLibrary` does nothing on netstandard2.1.**
  `NativeLibrary.SetDllImportResolver` does not exist there — Unity's Mono and
  IL2CPP — so `TryRegisterResolver` answers false having done nothing, and the
  `[DllImport]` name is what Unity matches against its own plugin import
  settings. It also answers false when the assembly already has a resolver,
  which the runtime allows only one of.
- **Nothing here is generated from your header.** You write the `[DllImport]`
  declarations, the handle subclasses, the table of domain codes and the
  P/Invoke signatures; this package supplies what those call into and no more.
- **A variable-length answer has to fit in one array.** `AbiBuffer` raises a
  binding failure above `Array.MaxLength` — 2,147,483,591 bytes — so an answer
  larger than that cannot be read through it.
- **A growing answer is given up on after four copies.** An answer larger on
  each of `AbiBuffer.Attempts` copies than when it was last measured is a
  binding failure rather than an unbounded retry.
- **Only two target frameworks are checked.** The content files land under
  `contentFiles/cs/any/`, so a project on any target framework restores them,
  and nothing verifies that they compile anywhere but `netstandard2.1` and
  `net8.0`.
- **There is no package feed to install from.** The `.nupkg` is not on
  nuget.org: you download it from a release into a folder in your own
  repository and name that folder as a source.

## Versioning

This is 0.1.1. Pre-1.0: a later 0.x release can change any name or signature.
Reference it by exact version (`Version="[0.1.1]"`), and move it together with
the foundation commit your Rust dependencies pin.

## Licence

Apache-2.0. See `LICENSE` and `NOTICE`.
