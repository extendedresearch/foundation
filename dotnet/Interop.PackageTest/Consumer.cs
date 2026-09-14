using System;
using ExtendedResearch.Interop;

namespace PackageTest
{
    /// <summary>A handle type, derived the way a package's binding derives one.</summary>
    internal sealed class ExampleHandle : AbiHandle
    {
        protected override void Destroy(IntPtr handle)
        {
        }
    }

    /// <summary>Reads a status the way a package's binding reads one.</summary>
    internal static class Consumer
    {
        internal static bool Succeeded(int status) => status == AbiCodes.Ok;
    }
}
