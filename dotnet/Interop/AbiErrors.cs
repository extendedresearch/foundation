// Part of the ExtendedResearch.Interop source package, built from
// dotnet/Interop in foundation. Edit it there, not in a consumer.
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
    /// and the boundary name (<c>EXAMPLE_ERR_NULL</c>); a domain code is the
    /// name the package declares (<c>EXAMPLE_ERR_TIMEOUT</c>), prefixed by
    /// <see cref="Token(string, string)"/> if the registration left the prefix
    /// off; any other code is <c>&lt;PREFIX&gt;_ERR_UNKNOWN</c>.</para>
    /// <para>Which exception, when the hook answers null or is absent:</para>
    /// <list type="table">
    /// <item><term>ERR_UTF8</term><description><see cref="ArgumentException"/>, with an <see cref="AbiException"/> inside</description></item>
    /// <item><term>ERR_RANGE</term><description><see cref="ArgumentOutOfRangeException"/>, with an <see cref="AbiException"/> inside</description></item>
    /// <item><term>ERR_PANIC</term><description><see cref="AbiPanicException"/></description></item>
    /// <item><term>anything else</term><description><see cref="AbiException"/></description></item>
    /// </list>
    /// <para>A failure of the binding rather than of the library is
    /// <see cref="AbiBindingException"/> unless the binding-failure hook
    /// answers otherwise. Every such failure goes through
    /// <see cref="BindingFailure(string, Exception?)"/>, so a package that
    /// passes the hook never lets an internal type reach its callers.</para>
    /// </remarks>
    internal sealed class AbiErrors
    {
        private readonly Dictionary<int, string> _domain;
        private readonly Func<int, string, string, Exception?>? _raise;
        private readonly Func<string, string, Exception?, Exception?>? _bindingFailure;

        /// <param name="prefix">The prefix every constant of the package begins with, such as <c>EXAMPLE</c>.</param>
        /// <param name="domainCodes">Every domain code with its full constant name.</param>
        /// <param name="raise">
        /// Consulted first for every failing code, with the code, its name and
        /// the message. Answer an exception to raise it, or null for the
        /// default mapping.
        /// </param>
        /// <param name="bindingFailure">
        /// Consulted for every failure of the binding — an answer that was not
        /// UTF-8, did not fit or kept growing, or a version mismatch — with
        /// <see cref="BindingCode"/>, the message, and the exception that
        /// caused it or null. Answer an exception to raise it, or null for
        /// <see cref="AbiBindingException"/>.
        /// </param>
        public AbiErrors(
            string prefix,
            IEnumerable<KeyValuePair<int, string>> domainCodes,
            Func<int, string, string, Exception?>? raise = null,
            Func<string, string, Exception?, Exception?>? bindingFailure = null)
        {
            Prefix = prefix;
            _domain = new Dictionary<int, string>();
            foreach (var entry in domainCodes)
            {
                _domain.Add(entry.Key, entry.Value);
            }
            _raise = raise;
            _bindingFailure = bindingFailure;
        }

        /// <summary>The prefix this translation was made with.</summary>
        public string Prefix { get; }

        /// <summary><c>&lt;PREFIX&gt;_ERR_BINDING</c>.</summary>
        public string BindingCode => Prefix + "_ERR_BINDING";

        /// <summary><c>&lt;PREFIX&gt;_ERR_UNKNOWN</c>.</summary>
        public string UnknownCode => Prefix + "_ERR_UNKNOWN";

        /// <summary>
        /// A constant's name as the package's header spells it: <paramref name="name"/>
        /// unchanged when it already carries <paramref name="prefix"/> and an
        /// underscore, and prefixed otherwise.
        /// </summary>
        /// <remarks>
        /// <para>The same step as <c>extendedresearch_status::codes::token</c>
        /// in Rust, which the Node and Python layers apply. A name that merely
        /// starts with the same letters is not the prefix, so
        /// <c>EXAMPLE0_ERR_X</c> under the prefix <c>EXAMPLE</c> becomes
        /// <c>EXAMPLE_EXAMPLE0_ERR_X</c> — deliberately ugly, because a package
        /// that produces it has named a constant no header declares.</para>
        /// <para>The rule is written in both languages and neither can call the
        /// other, so it is registered as a <c>[[shared_rule]]</c> in
        /// <c>ecosystem/PACKAGES.toml</c>, and
        /// <c>crates/status/vectors/0001-an-error-token-is-the-header-spelling-of-a-constant.json</c>
        /// is the file both run against.
        /// <c>docs/conventions/error-tokens.md</c> states the grammar.</para>
        /// </remarks>
        public static string Token(string prefix, string name)
        {
            if (name.Length > prefix.Length
                && name.StartsWith(prefix, StringComparison.Ordinal)
                && name[prefix.Length] == '_')
            {
                return name;
            }
            return prefix + "_" + name;
        }

        /// <summary>The header's name for a code.</summary>
        /// <remarks>
        /// A domain name goes through <see cref="Token(string, string)"/> like
        /// a boundary one. A package that registered a domain code under an
        /// unprefixed name gets the header's spelling rather than the bare
        /// name, which is no constant any header declares.
        /// </remarks>
        public string NameOf(int code)
        {
            if (code == AbiCodes.Ok)
            {
                return Token(Prefix, "OK");
            }
            var boundary = AbiCodes.BoundaryName(code);
            if (boundary != null)
            {
                return Token(Prefix, boundary);
            }
            return _domain.TryGetValue(code, out var name) ? Token(Prefix, name) : UnknownCode;
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
        public Exception BindingFailure(string detail) => BindingFailure(detail, null);

        /// <summary>A failure of the binding, caused by <paramref name="inner"/> when it is not null.</summary>
        public Exception BindingFailure(string detail, Exception? inner)
        {
            var raised = _bindingFailure?.Invoke(BindingCode, detail, inner);
            if (raised != null)
            {
                return raised;
            }
            return inner == null
                ? new AbiBindingException(BindingCode, detail)
                : new AbiBindingException(BindingCode, detail, inner);
        }

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
