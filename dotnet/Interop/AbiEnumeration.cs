// Part of the ExtendedResearch.Interop source package, built from
// dotnet/Interop in foundation. Edit it there, not in a consumer.
#nullable enable

using System.Collections.Generic;

namespace ExtendedResearch.Interop
{
    /// <summary>An enumeration's <c>_count</c>.</summary>
    internal delegate int CountCall(out uint count);

    /// <summary>An enumeration's <c>_at</c>.</summary>
    internal delegate int AtCall(uint index, out int value);

    /// <summary>An enumeration's <c>_name</c>.</summary>
    internal delegate int NameCall(int value, byte[]? destination, ulong capacity, out ulong length);

    /// <summary>One value of an enumeration with the contract's name for it.</summary>
    internal readonly struct AbiMember
    {
        public AbiMember(int value, string name)
        {
            Value = value;
            Name = name;
        }

        /// <summary>The value that crosses the boundary.</summary>
        public int Value { get; }

        /// <summary>The contract's name for it.</summary>
        public string Name { get; }
    }

    /// <summary>Reading an enumeration out of the library rather than transcribing it.</summary>
    /// <remarks>
    /// A binding that loops over <c>_count</c> and <c>_at</c> cannot produce a
    /// subset of the library's values; a binding that transcribes them can, and
    /// the failure is silent.
    /// </remarks>
    internal static class AbiEnumeration
    {
        /// <summary>Every value with its name, in <c>_at</c> order.</summary>
        /// <param name="count">The library's <c>_count</c>.</param>
        /// <param name="at">The library's <c>_at</c>.</param>
        /// <param name="name">The library's <c>_name</c>.</param>
        /// <param name="errors">The package's error translation.</param>
        /// <param name="what">The enumeration, for messages.</param>
        public static IReadOnlyList<AbiMember> Read(
            CountCall count, AtCall at, NameCall name, AbiErrors errors, string what)
        {
            errors.Check(count(out var total), what + "_count");
            var members = new List<AbiMember>((int)System.Math.Min(total, 1024u));
            for (uint index = 0; index < total; index++)
            {
                errors.Check(at(index, out var value), what + "_at");
                var spelled = AbiBuffer.ReadText(
                    (byte[]? destination, ulong capacity, out ulong length) =>
                        name(value, destination, capacity, out length),
                    errors,
                    what + "_name");
                members.Add(new AbiMember(value, spelled));
            }
            return members;
        }
    }
}
