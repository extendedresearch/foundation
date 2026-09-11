// Vendored from foundation's dotnet/Interop, carried by the Rust crate
// extendedresearch-interop-sources. Edit it there; the package's drift test
// fails when this copy differs.
#nullable enable

using System;
using System.Runtime.InteropServices;

namespace ExtendedResearch.Interop
{
    /// <summary>
    /// An opaque handle a library created, freed by the library's
    /// <c>_destroy</c> exactly once.
    /// </summary>
    /// <remarks>
    /// <para>Derive one sealed class per handle type and call the matching
    /// <c>_destroy</c> from <see cref="Destroy"/>:</para>
    /// <code>
    /// internal sealed class StreamHandle : AbiHandle
    /// {
    ///     protected override void Destroy(IntPtr handle) => Native.ranvier_stream_destroy(handle);
    /// }
    /// </code>
    /// <para>P/Invoke declarations then take and answer the derived type
    /// (<c>out StreamHandle stream</c>), and the marshaller keeps the handle
    /// alive for the length of each call.</para>
    /// <para><b>The finalizer rule.</b> <see cref="Destroy"/> runs from
    /// <see cref="SafeHandle.Dispose()"/> or from the finalizer thread, in an
    /// order nothing controls, which is why every <c>_destroy</c> in the
    /// ecosystem accepts null, does not block, and may be called from any
    /// thread. <see cref="SafeHandle"/> calls <see cref="ReleaseHandle"/> once,
    /// and not at all for a handle that is <see cref="IsInvalid"/>.</para>
    /// </remarks>
    internal abstract class AbiHandle : SafeHandle
    {
        protected AbiHandle()
            : base(IntPtr.Zero, ownsHandle: true)
        {
        }

        /// <summary>Null is the one value no <c>_create</c> answers for a live object.</summary>
        public override bool IsInvalid => handle == IntPtr.Zero;

        /// <summary>Call the library's <c>_destroy</c> for this handle type.</summary>
        /// <remarks>Must not throw: it can run on the finalizer thread.</remarks>
        protected abstract void Destroy(IntPtr handle);

        protected sealed override bool ReleaseHandle()
        {
            Destroy(handle);
            return true;
        }
    }
}
