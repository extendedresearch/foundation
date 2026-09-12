// Vendored from foundation's dotnet/Interop, carried by the Rust crate
// extendedresearch-interop-sources. Edit it there; the package's drift test
// fails when this copy differs.
#nullable enable

using System;
using System.Text;

namespace ExtendedResearch.Interop
{
    /// <summary>
    /// One call in the measure-then-copy shape, bound to its handle:
    /// <c>(d, c, out l) =&gt; Native.stream_name(stream, d, c, out l)</c>.
    /// </summary>
    /// <remarks>
    /// Declare the native side's destination as <c>[Out] byte[]? destination</c>.
    /// A null array is a null pointer, which measures; a <c>byte[]</c> is
    /// pinned for the call, so the library writes straight into it.
    /// </remarks>
    internal delegate int CopyCall(byte[]? destination, ulong capacity, out ulong length);

    /// <summary>
    /// Reading a variable-length answer, and passing text in.
    /// </summary>
    /// <remarks>
    /// The same reading <c>extendedresearch-abi</c>'s <c>binding</c> module does
    /// from Rust: measure with a null destination, allocate, copy. An answer
    /// that grew between the measuring call and the copying one is refused with
    /// ERR_RANGE and the new size, so the read measures again rather than fails,
    /// up to <see cref="Attempts"/> times. <c>*out_len</c> never counts a
    /// string's terminator and the capacity always must.
    /// </remarks>
    internal static class AbiBuffer
    {
        /// <summary>How many copies of a growing answer are tried before giving up.</summary>
        public const int Attempts = 4;

        /// <summary>The longest byte array the runtime allocates (<c>Array.MaxLength</c> on .NET 6 and later).</summary>
        private const ulong MaxLength = 0x7FFFFFC7;

        private static readonly UTF8Encoding Strict = new UTF8Encoding(false, true);

        /// <summary>Read text a library answers.</summary>
        /// <exception cref="AbiBindingException">The answer was not UTF-8, did not fit, or kept growing, unless the binding-failure hook answers the package's own type.</exception>
        public static string ReadText(CopyCall call, AbiErrors errors, string what)
        {
            var bytes = Read(call, 1, errors, what);
            try
            {
                return Strict.GetString(bytes);
            }
            catch (DecoderFallbackException error)
            {
                throw errors.BindingFailure(what + " answered text that is not UTF-8", error);
            }
        }

        /// <summary>Read a byte run a library answers.</summary>
        /// <exception cref="AbiBindingException">The answer did not fit, or kept growing, unless the binding-failure hook answers the package's own type.</exception>
        public static byte[] ReadBytes(CopyCall call, AbiErrors errors, string what) =>
            Read(call, 0, errors, what);

        /// <summary>
        /// Text as the null-terminated UTF-8 a <c>const char *</c> parameter takes.
        /// </summary>
        /// <exception cref="ArgumentException">
        /// The text contains a null character, which would end it early, or a
        /// lone surrogate, which has no UTF-8 form.
        /// </exception>
        public static byte[] ToUtf8Z(string value, string what)
        {
            if (value.IndexOf('\0') >= 0)
            {
                throw new ArgumentException(what + " contains a null character, which cannot cross the C ABI");
            }
            try
            {
                var bytes = new byte[Strict.GetByteCount(value) + 1];
                Strict.GetBytes(value, 0, value.Length, bytes, 0);
                return bytes;
            }
            catch (EncoderFallbackException error)
            {
                throw new ArgumentException(what + " is not valid Unicode and has no UTF-8 form", error);
            }
        }

        private static byte[] Read(CopyCall call, ulong terminator, AbiErrors errors, string what)
        {
            errors.Check(call(null, 0, out var needed), what);
            for (var attempt = 0; attempt < Attempts; attempt++)
            {
                if (needed > MaxLength - terminator)
                {
                    throw errors.BindingFailure(what + " answered a length of " + needed + ", which this runtime cannot allocate");
                }
                var capacity = needed + terminator;
                if (capacity == 0)
                {
                    return Array.Empty<byte>();
                }
                var buffer = new byte[capacity];
                var status = call(buffer, capacity, out var written);
                if (status == AbiCodes.Ok)
                {
                    if (written > needed)
                    {
                        throw errors.BindingFailure(what + " wrote " + written + " bytes after measuring " + needed);
                    }
                    Array.Resize(ref buffer, (int)written);
                    return buffer;
                }
                if (status != AbiCodes.ErrRange)
                {
                    throw errors.ToException(status, what);
                }
                needed = written;
            }
            throw errors.BindingFailure(what + " grew on each of " + Attempts + " attempts to copy it");
        }
    }
}
