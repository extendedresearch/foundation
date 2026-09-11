// Vendored from foundation's dotnet/Interop, carried by the Rust crate
// extendedresearch-interop-sources. Edit it there; the package's drift test
// fails when this copy differs.
#nullable enable

namespace ExtendedResearch.Interop
{
    /// <summary>
    /// The boundary codes every library in the ecosystem answers, as
    /// <c>extendedresearch-abi</c>'s <c>codes</c> module defines them.
    /// </summary>
    /// <remarks>
    /// Zero is success and every other value is a failure, including one this
    /// build has never heard of. Codes from -1 to -15 belong to the boundary;
    /// a library numbers its own from <see cref="DomainFloor"/> down. A Rust
    /// test in <c>extendedresearch-interop-sources</c> compares each value and
    /// name here against the Rust constants.
    /// </remarks>
    internal static class AbiCodes
    {
        /// <summary>The call succeeded.</summary>
        public const int Ok = 0;

        /// <summary>A required handle or out-pointer was null.</summary>
        public const int ErrNull = -1;

        /// <summary>A buffer was too small, or an index or length was past what the call can reach.</summary>
        public const int ErrRange = -2;

        /// <summary>Text crossing the boundary was not valid UTF-8.</summary>
        public const int ErrUtf8 = -3;

        /// <summary>A panic was caught at the boundary; the library's state is unknown.</summary>
        public const int ErrPanic = -4;

        /// <summary>The object is in the wrong state for the call.</summary>
        public const int ErrState = -5;

        /// <summary>The most negative code the boundary reserves; a library's own codes are at and below it.</summary>
        public const int DomainFloor = -16;

        /// <summary>Whether a code is one the boundary defines.</summary>
        public static bool IsBoundary(int code) => code < 0 && code > DomainFloor;

        /// <summary>Whether a code belongs to the library rather than the boundary.</summary>
        public static bool IsDomain(int code) => code <= DomainFloor;

        /// <summary>
        /// The boundary name of a code, without the library's prefix, or null
        /// for success and for any code the boundary does not name.
        /// </summary>
        public static string? BoundaryName(int code)
        {
            switch (code)
            {
                case ErrNull: return "ERR_NULL";
                case ErrRange: return "ERR_RANGE";
                case ErrUtf8: return "ERR_UTF8";
                case ErrPanic: return "ERR_PANIC";
                case ErrState: return "ERR_STATE";
                default: return null;
            }
        }
    }
}
