using System;
using System.Linq;
using System.Runtime.CompilerServices;
using System.Text;
using Xunit;

namespace ExtendedResearch.Interop.Tests;

public class InteropTests
{
    private static readonly AbiErrors Errors = Testlib.Errors;

    [Fact]
    public void TheResolverIsRegisteredOnceAndOnlyOnce()
    {
        Assert.True(AbiLibrary.CanResolve);
        Assert.True(Testlib.Registered);
        Assert.False(AbiLibrary.TryRegisterResolver(typeof(Testlib).Assembly, Native.Library, () => null));
        Assert.NotNull(Testlib.Locate());
    }

    [Fact]
    public void TheVersionMustMatchExactly()
    {
        Errors.RequireVersion(1, Native.testlib_abi_version(), "testlib");
        var mismatch = Assert.Throws<AbiBindingException>(
            () => Errors.RequireVersion(2, Native.testlib_abi_version(), "testlib"));
        Assert.Equal("TESTLIB_ERR_BINDING", mismatch.Name);
        Assert.Equal(0, mismatch.Code);
    }

    [Fact]
    public void TextAndBytesAreReadInTheMeasureThenCopyShape()
    {
        using var counter = Testlib.Create("Zähler ✓", 3);
        var name = AbiBuffer.ReadText(
            (byte[]? d, ulong c, out ulong l) => Native.testlib_counter_name(counter, d, c, out l),
            Errors,
            "testlib_counter_name");
        Assert.Equal("Zähler ✓", name);

        var bytes = AbiBuffer.ReadBytes(
            (byte[]? d, ulong c, out ulong l) => Native.testlib_counter_name_bytes(counter, d, c, out l),
            Errors,
            "testlib_counter_name_bytes");
        Assert.Equal(Encoding.UTF8.GetBytes("Zähler ✓"), bytes);
    }

    [Fact]
    public void DisposeDestroysOnceAndOnlyOnce()
    {
        var before = Native.testlib_destroyed_count();
        var counter = Testlib.Create("disposed", 1);
        counter.Dispose();
        counter.Dispose();
        Assert.Equal(before + 1, Native.testlib_destroyed_count());
        Assert.True(counter.IsClosed);
    }

    [MethodImpl(MethodImplOptions.NoInlining)]
    private static void CreateAndDrop() => Testlib.Create("abandoned", 1);

    [Fact]
    public void TheFinalizerDestroysAnAbandonedHandle()
    {
        var before = Native.testlib_destroyed_count();
        for (var i = 0; i < 8; i++)
        {
            CreateAndDrop();
        }
        for (var round = 0; round < 10 && Native.testlib_destroyed_count() < before + 8; round++)
        {
            GC.Collect();
            GC.WaitForPendingFinalizers();
        }
        // Other tests destroy counters concurrently, so at least eight more.
        Assert.True(Native.testlib_destroyed_count() >= before + 8);
    }

    [Fact]
    public void ADomainCodeIsAnAbiExceptionWithTheDeclaredName()
    {
        var empty = Assert.Throws<AbiException>(() => Testlib.Create("", 1));
        Assert.Equal(Testlib.ErrEmptyName, empty.Code);
        Assert.Equal("TESTLIB_ERR_EMPTY_NAME", empty.Name);
        Assert.Contains("TESTLIB_ERR_EMPTY_NAME (-16)", empty.Message);
    }

    private sealed class FullException : Exception
    {
        public FullException(string message) : base(message) { }
    }

    [Fact]
    public void TheHookRaisesThePackagesOwnTypeAndDefersOtherwise()
    {
        var hooked = new AbiErrors(
            "TESTLIB",
            new[] { new System.Collections.Generic.KeyValuePair<int, string>(Testlib.ErrFull, "TESTLIB_ERR_FULL") },
            (code, name, message) => code == Testlib.ErrFull ? new FullException(message) : null);
        using var counter = Testlib.Create("limited", 1);
        hooked.Check(Native.testlib_counter_increment(counter, out var first), "increment");
        Assert.Equal(1u, first);
        Assert.Throws<FullException>(
            () => hooked.Check(Native.testlib_counter_increment(counter, out _), "increment"));

        Errors.Check(Native.testlib_counter_close(counter), "close");
        var state = Assert.Throws<AbiException>(
            () => hooked.Check(Native.testlib_counter_increment(counter, out _), "increment"));
        Assert.Equal("TESTLIB_ERR_STATE", state.Name);
    }

    private sealed class BindingException : Exception
    {
        public BindingException(string name, string message, Exception? inner)
            : base(message, inner)
        {
            Name = name;
        }

        public string Name { get; }
    }

    [Fact]
    public void TheBindingHookRaisesThePackagesOwnTypeForEveryBindingFailure()
    {
        var hooked = new AbiErrors(
            "TESTLIB",
            Array.Empty<System.Collections.Generic.KeyValuePair<int, string>>(),
            bindingFailure: (name, message, inner) => new BindingException(name, message, inner));

        var mismatch = Assert.Throws<BindingException>(
            () => hooked.RequireVersion(2, Native.testlib_abi_version(), "testlib"));
        Assert.Equal("TESTLIB_ERR_BINDING", mismatch.Name);
        Assert.Null(mismatch.InnerException);

        CopyCall invalid = (byte[]? d, ulong c, out ulong l) =>
        {
            l = 1;
            if (d != null)
            {
                d[0] = 0xFF;
                d[1] = 0;
            }
            return AbiCodes.Ok;
        };
        var notUtf8 = Assert.Throws<BindingException>(() => AbiBuffer.ReadText(invalid, hooked, "invalid"));
        Assert.IsAssignableFrom<DecoderFallbackException>(notUtf8.InnerException);

        Assert.Throws<BindingException>(
            () => AbiBuffer.ReadText(Growing("abcdefghijklmnop", 12), hooked, "growing"));
    }

    [Fact]
    public void APanicIsAnAbiPanicException()
    {
        var panic = Assert.Throws<AbiPanicException>(() => Errors.Check(Native.testlib_panic(), "panic"));
        Assert.Equal(AbiCodes.ErrPanic, panic.Code);
        Assert.Equal("TESTLIB_ERR_PANIC", panic.Name);
    }

    [Fact]
    public void Utf8IsAnArgumentExceptionCarryingTheCode()
    {
        var status = Native.testlib_counter_create(new byte[] { 0xFF, 0 }, 1, out var counter);
        Assert.True(counter.IsInvalid);
        var error = Assert.Throws<ArgumentException>(() => Errors.Check(status, "create"));
        var inner = Assert.IsType<AbiException>(error.InnerException);
        Assert.Equal(AbiCodes.ErrUtf8, inner.Code);
        Assert.Equal("TESTLIB_ERR_UTF8", inner.Name);
    }

    [Fact]
    public void ANullArgumentIsTheBaseException()
    {
        var status = Native.testlib_counter_create(null, 1, out var counter);
        Assert.True(counter.IsInvalid);
        var error = Assert.Throws<AbiException>(() => Errors.Check(status, "create"));
        Assert.Equal("TESTLIB_ERR_NULL", error.Name);
    }

    [Fact]
    public void AnEnumerationIsReadByLooping()
    {
        var members = AbiEnumeration.Read(
            Native.testlib_color_count,
            Native.testlib_color_at,
            Native.testlib_color_name,
            Errors,
            "testlib_color");
        Assert.Equal(
            new[] { (0, "COLOR_RED"), (1, "COLOR_GREEN"), (7, "COLOR_BLUE") },
            members.Select(m => (m.Value, m.Name)).ToArray());
    }

    [Fact]
    public void AValueTheEnumerationLacksIsOutOfRange()
    {
        var error = Assert.Throws<ArgumentOutOfRangeException>(() => AbiBuffer.ReadText(
            (byte[]? d, ulong c, out ulong l) => Native.testlib_color_name(3, d, c, out l),
            Errors,
            "testlib_color_name"));
        Assert.Equal(AbiCodes.ErrRange, Assert.IsType<AbiException>(error.InnerException).Code);
    }

    [Fact]
    public void AnUnknownCodeIsNamedUnknown()
    {
        Assert.Equal("TESTLIB_ERR_UNKNOWN", Errors.NameOf(-99));
        Assert.Equal("TESTLIB_ERR_UNKNOWN", Errors.NameOf(-7));
        Assert.Equal("TESTLIB_OK", Errors.NameOf(0));
        var error = Assert.Throws<AbiException>(() => Errors.Check(-99, "call"));
        Assert.Equal(-99, error.Code);
    }

    /// <summary>A library whose answer grows <paramref name="growths"/> times after being measured.</summary>
    private static CopyCall Growing(string final, int growths)
    {
        var calls = 0;
        return (byte[]? destination, ulong capacity, out ulong length) =>
        {
            var current = final.Substring(0, Math.Max(0, final.Length - Math.Max(0, growths - calls)));
            calls++;
            var bytes = Encoding.UTF8.GetBytes(current);
            length = (ulong)bytes.Length;
            if (destination == null)
            {
                return AbiCodes.Ok;
            }
            if (capacity < (ulong)bytes.Length + 1)
            {
                return AbiCodes.ErrRange;
            }
            bytes.CopyTo(destination, 0);
            destination[bytes.Length] = 0;
            return AbiCodes.Ok;
        };
    }

    [Fact]
    public void AnAnswerThatGrewIsMeasuredAgain()
    {
        Assert.Equal("abcdef", AbiBuffer.ReadText(Growing("abcdef", 2), Errors, "growing"));
    }

    [Fact]
    public void AnAnswerThatKeepsGrowingIsRefused()
    {
        var error = Assert.Throws<AbiBindingException>(
            () => AbiBuffer.ReadText(Growing("abcdefghijklmnop", 12), Errors, "growing"));
        Assert.Contains("grew on each of 4 attempts", error.Message);
    }

    [Fact]
    public void AnEmptyAnswerIsEmpty()
    {
        CopyCall empty = (byte[]? d, ulong c, out ulong l) =>
        {
            l = 0;
            if (d != null && c > 0)
            {
                d[0] = 0;
            }
            return AbiCodes.Ok;
        };
        Assert.Equal("", AbiBuffer.ReadText(empty, Errors, "empty"));
        Assert.Empty(AbiBuffer.ReadBytes(empty, Errors, "empty"));
    }

    [Fact]
    public void TextThatIsNotUtf8IsABindingFailure()
    {
        CopyCall invalid = (byte[]? d, ulong c, out ulong l) =>
        {
            l = 1;
            if (d != null)
            {
                d[0] = 0xFF;
                d[1] = 0;
            }
            return AbiCodes.Ok;
        };
        Assert.Throws<AbiBindingException>(() => AbiBuffer.ReadText(invalid, Errors, "invalid"));
    }

    [Fact]
    public void TextInIsNullTerminatedUtf8AndRefusesWhatCannotCross()
    {
        Assert.Equal(new byte[] { 0x61, 0xC3, 0xA4, 0 }, AbiBuffer.ToUtf8Z("aä", "name"));
        Assert.Throws<ArgumentException>(() => AbiBuffer.ToUtf8Z("a\0b", "name"));
        Assert.Throws<ArgumentException>(() => AbiBuffer.ToUtf8Z("\uD800", "name"));
    }
}
