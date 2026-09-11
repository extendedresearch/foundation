// Vendored from foundation's dotnet/Interop, carried by the Rust crate
// extendedresearch-interop-sources. Edit it there; the package's drift test
// fails when this copy differs.
#nullable enable

using System;
using System.Collections.Generic;

namespace ExtendedResearch.Interop
{
    /// <summary>
    /// Builds the exception one package's status code calls for.
    /// </summary>
    /// <remarks>
    /// <para>Names follow the header: a boundary code is the package's prefix
    /// and the boundary name (<c>RANVIER_ERR_NULL</c>); a domain code is the
    /// name the package declares (<c>RANVIER_ERR_TIMEOUT</c>); any other code
    /// is <c>&lt;PREFIX&gt;_ERR_UNKNOWN</c>.</para>
    /// <para>Which exception, when the hook answers null or is absent:</para>
    /// <list type="table">
    /// <item><term>ERR_UTF8</term><description><see cref="ArgumentException"/>, with an <see cref="AbiException"/> inside</description></item>
    /// <item><term>ERR_RANGE</term><description><see cref="ArgumentOutOfRangeException"/>, with an <see cref="AbiException"/> inside</description></item>
    /// <item><term>ERR_PANIC</term><description><see cref="AbiPanicException"/></description></item>
    /// <item><term>anything else</term><description><see cref="AbiException"/></description></item>
    /// </list>
    /// </remarks>
    internal sealed class AbiErrors
    {
        private readonly Dictionary<int, string> _domain;
        private readonly Func<int, string, string, Exception?>? _raise;

        /// <param name="prefix">The prefix every constant of the package begins with, such as <c>RANVIER</c>.</param>
        /// <param name="domainCodes">Every domain code with its full constant name.</param>
        /// <param name="raise">
        /// Consulted first for every failing code, with the code, its name and
        /// the message. Answer an exception to raise it, or null for the
        /// default mapping.
        /// </param>
        public AbiErrors(
            string prefix,
            IEnumerable<KeyValuePair<int, string>> domainCodes,
            Func<int, string, string, Exception?>? raise = null)
        {
            Prefix = prefix;
            _domain = new Dictionary<int, string>();
            foreach (var entry in domainCodes)
            {
                _domain.Add(entry.Key, entry.Value);
            }
            _raise = raise;
        }

        /// <summary>The prefix this translation was made with.</summary>
        public string Prefix { get; }

        /// <summary><c>&lt;PREFIX&gt;_ERR_BINDING</c>.</summary>
        public string BindingCode => Prefix + "_ERR_BINDING";

        /// <summary><c>&lt;PREFIX&gt;_ERR_UNKNOWN</c>.</summary>
        public string UnknownCode => Prefix + "_ERR_UNKNOWN";

        /// <summary>The header's name for a code.</summary>
        public string NameOf(int code)
        {
            if (code == AbiCodes.Ok)
            {
                return Prefix + "_OK";
            }
            var boundary = AbiCodes.BoundaryName(code);
            if (boundary != null)
            {
                return Prefix + "_" + boundary;
            }
            return _domain.TryGetValue(code, out var name) ? name : UnknownCode;
        }

        /// <summary>Return for success; throw what <see cref="ToException"/> builds otherwise.</summary>
        /// <param name="status">What the library answered.</param>
        /// <param name="call">The operation, for the message.</param>
        public void Check(int status, string call)
        {
            if (status != AbiCodes.Ok)
            {
                throw ToException(status, call);
            }
        }

        /// <summary>The exception a failing status calls for.</summary>
        public Exception ToException(int status, string call)
        {
            var name = NameOf(status);
            var message = call + " answered " + name + " (" + status + ")";
            var raised = _raise?.Invoke(status, name, message);
            if (raised != null)
            {
                return raised;
            }
            switch (status)
            {
                case AbiCodes.ErrUtf8:
                    return new ArgumentException(message, new AbiException(status, name, message));
                case AbiCodes.ErrRange:
                    return new ArgumentOutOfRangeException(message, new AbiException(status, name, message));
                case AbiCodes.ErrPanic:
                    return new AbiPanicException(status, name, message);
                default:
                    return new AbiException(status, name, message);
            }
        }

        /// <summary>A failure of the binding rather than of the library.</summary>
        public AbiBindingException BindingFailure(string detail) =>
            new AbiBindingException(BindingCode, detail);

        /// <summary>A failure of the binding, caused by <paramref name="inner"/>.</summary>
        public AbiBindingException BindingFailure(string detail, Exception inner) =>
            new AbiBindingException(BindingCode, detail, inner);

        /// <summary>
        /// Throw unless the library implements exactly the ABI version the
        /// binding was built against.
        /// </summary>
        /// <remarks>
        /// Equality, not "at least": the conventions are an ownership contract,
        /// not a feature set a later version is a superset of.
        /// </remarks>
        public void RequireVersion(uint expected, uint actual, string library)
        {
            if (expected != actual)
            {
                throw BindingFailure(
                    "this binding was built against " + library + " ABI version " + expected
                    + " and loaded a library implementing version " + actual
                    + "; they must match exactly");
            }
        }
    }
}
