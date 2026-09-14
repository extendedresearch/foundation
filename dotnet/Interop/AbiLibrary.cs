// Part of the ExtendedResearch.Interop source package, built from
// dotnet/Interop in foundation. Edit it there, not in a consumer.
#nullable enable

using System;
using System.Reflection;
#if NETCOREAPP3_0_OR_GREATER
using System.Runtime.InteropServices;
#endif

namespace ExtendedResearch.Interop
{
    /// <summary>
    /// Pointing <c>[DllImport]</c> at a native library the default search
    /// would not find.
    /// </summary>
    /// <remarks>
    /// <para>On .NET Core 3.0 and later, including net8.0, this registers a
    /// resolver with <c>NativeLibrary.SetDllImportResolver</c>. On
    /// netstandard2.1 — Unity's Mono and IL2CPP — that API does not exist, and
    /// <see cref="TryRegisterResolver"/> answers false having done nothing:
    /// Unity resolves a plugin from its own import settings, and the
    /// <c>[DllImport]</c> name is what it matches.</para>
    /// <para>Call it once, before the first P/Invoke into the library, from
    /// the package's static constructor or module initializer.</para>
    /// </remarks>
    internal static class AbiLibrary
    {
        /// <summary>Whether this build can register a resolver at all.</summary>
        public static bool CanResolve
        {
            get
            {
#if NETCOREAPP3_0_OR_GREATER
                return true;
#else
                return false;
#endif
            }
        }

        /// <summary>
        /// Resolve <paramref name="libraryName"/> for <paramref name="assembly"/>
        /// to the path <paramref name="locate"/> answers.
        /// </summary>
        /// <param name="assembly">The assembly whose <c>[DllImport]</c>s to resolve.</param>
        /// <param name="libraryName">The name the <c>[DllImport]</c> attributes use.</param>
        /// <param name="locate">
        /// The full path to load, or null to fall back to the default search.
        /// Called on first use, not now.
        /// </param>
        /// <returns>
        /// True when registered. False on netstandard2.1, and when the assembly
        /// already has a resolver, which the runtime allows only one of.
        /// </returns>
        public static bool TryRegisterResolver(Assembly assembly, string libraryName, Func<string?> locate)
        {
#if NETCOREAPP3_0_OR_GREATER
            try
            {
                NativeLibrary.SetDllImportResolver(
                    assembly,
                    (name, requesting, searchPath) =>
                    {
                        if (name != libraryName)
                        {
                            return IntPtr.Zero;
                        }
                        var path = locate();
                        return path != null && NativeLibrary.TryLoad(path, out var loaded) ? loaded : IntPtr.Zero;
                    });
                return true;
            }
            catch (InvalidOperationException)
            {
                return false;
            }
#else
            return false;
#endif
        }
    }
}
