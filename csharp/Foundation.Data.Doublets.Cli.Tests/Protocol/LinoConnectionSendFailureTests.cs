using Foundation.Data.Doublets.Cli.Protocol;
using Link.Foundation.Links.Notation.Binary;

namespace Foundation.Data.Doublets.Cli.Tests.Protocol;

public class LinoConnectionSendFailureTests
{
    private sealed class FailingTransport(bool failFlush, Exception failure) : MemoryStream
    {
        public int Writes { get; private set; }
        public int Flushes { get; private set; }
        public override void Write(byte[] buffer, int offset, int count)
        {
            Writes++;
            if (!failFlush) throw failure;
            base.Write(buffer, offset, count);
        }
        public override void Flush()
        {
            Flushes++;
            throw failure;
        }
    }

    [Theory]
    [InlineData(false, false)]
    [InlineData(false, true)]
    [InlineData(true, false)]
    [InlineData(true, true)]
    public void SendPreservesTransportFailureAtWriteAndFlush(bool failFlush, bool disposed)
    {
        Exception failure = disposed
            ? new ObjectDisposedException("transport", "injected send failure")
            : new IOException("injected send failure");
        using var transport = new FailingTransport(failFlush, failure);
        using var connection = new LinoConnection(transport, new TextLinoProtocol());
        var error = Assert.Throws<LinoProtocolException>(() => connection.Send(LinoFormat.ParseDocument("() ()")));
        Assert.Equal(LinoProtocolErrorKind.Io, error.Kind);
        Assert.Equal(failure.Message, error.Detail);
        Assert.Same(failure, error.InnerException);
        Assert.Equal(1, transport.Writes);
        Assert.Equal(failFlush ? 1 : 0, transport.Flushes);
        Assert.Equal(failFlush, transport.Length > 0);
    }
}
