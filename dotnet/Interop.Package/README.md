# ExtendedResearch.Interop

C# source that a package's .NET binding compiles into its own assembly, for
`netstandard2.1` and `net8.0`:

| Type | What it does |
|---|---|
| `AbiHandle` | A `SafeHandle` whose release calls the library's `_destroy` |
| `AbiErrors`, `AbiException` | A status code to the package's exception |
| `AbiBuffer` | Measure-then-copy reads of text and bytes |
| `AbiLibrary` | A `DllImport` resolver, on net8.0 |
| `AbiEnumeration` | An enumeration's `_count`/`_at`/`_name` read into a list |
| `AbiCodes` | The boundary codes |

Every type is `internal`, in the namespace `ExtendedResearch.Interop`.

## What the package holds

Source files and no assembly. Each file is under
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

`dotnet/Interop.PackageTest` in foundation is a consumer of exactly this shape:
it restores the package from a local folder and builds for `netstandard2.1` and
`net8.0` with C# 8 and warnings as errors.

## Status

0.1.1. A later 0.x release can change any name or signature. Licensed under
Apache-2.0. See `LICENSE` and `NOTICE`.
