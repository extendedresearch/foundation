// Vendored from foundation's dotnet/Interop, carried by the Rust crate
// extendedresearch-interop-sources. Edit it there; the package's drift test
// fails when this copy differs.
#nullable enable

using System;

namespace ExtendedResearch.Interop
{
    /// <summary>
    /// A failure a library answered, carrying the code and the constant's name.
    /// </summary>
    /// <remarks>
    /// These types are <c>internal</c> to the package assembly that compiles
    /// them, so a caller outside it can catch them only as
    /// <see cref="Exception"/>. A package whose callers should catch by type
    /// passes <see cref="AbiErrors"/> two hooks that answer the package's own
    /// public exception types: one for a failing code, one for a failure of
    /// the binding.
    /// </remarks>
    internal class AbiException : Exception
    {
        public AbiException(int code, string name, string message)
            : base(message)
        {
            Code = code;
            Name = name;
        }

        public AbiException(int code, string name, string message, Exception inner)
            : base(message, inner)
        {
            Code = code;
            Name = name;
        }

        /// <summary>The <c>int32_t</c> the library answered; zero for a binding failure.</summary>
        public int Code { get; }

        /// <summary>The constant's name as the header spells it, such as <c>CA3_ERR_TRUNCATED</c>.</summary>
        public string Name { get; }
    }

    /// <summary>
    /// The library caught a panic. Its state is unknown, and nothing else
    /// should be called on it.
    /// </summary>
    internal sealed class AbiPanicException : AbiException
    {
        public AbiPanicException(int code, string name, string message)
            : base(code, name, message)
        {
        }
    }

    /// <summary>
    /// Something the binding could not do, rather than a failure the library
    /// answered: an answer a conforming library never gives, or a version
    /// mismatch. <see cref="AbiException.Code"/> is zero and
    /// <see cref="AbiException.Name"/> is <c>&lt;PREFIX&gt;_ERR_BINDING</c>.
    /// </summary>
    internal sealed class AbiBindingException : AbiException
    {
        public AbiBindingException(string name, string message)
            : base(AbiCodes.Ok, name, message)
        {
        }

        public AbiBindingException(string name, string message, Exception inner)
            : base(AbiCodes.Ok, name, message, inner)
        {
        }
    }
}
