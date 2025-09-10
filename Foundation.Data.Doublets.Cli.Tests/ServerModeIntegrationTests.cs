using Xunit;
using System.Diagnostics;
using System.IO;

namespace Foundation.Data.Doublets.Cli.Tests
{
    public class ServerModeIntegrationTests : IDisposable
    {
        private string _tempDbFile;

        public ServerModeIntegrationTests()
        {
            _tempDbFile = Path.GetTempFileName();
        }

        public void Dispose()
        {
            if (File.Exists(_tempDbFile))
            {
                File.Delete(_tempDbFile);
            }
        }

        [Fact]
        public void Should_Show_Server_Option_In_Help()
        {
            // Arrange & Act
            var process = new Process
            {
                StartInfo = new ProcessStartInfo
                {
                    FileName = "dotnet",
                    Arguments = $"run --project {GetCliProjectPath()} -- --help",
                    UseShellExecute = false,
                    RedirectStandardOutput = true,
                    RedirectStandardError = true
                }
            };

            process.Start();
            var output = process.StandardOutput.ReadToEnd();
            process.WaitForExit();

            // Assert
            Assert.Contains("--server", output);
            Assert.Contains("Start server listening on a port", output);
            Assert.Contains("--port", output);
            Assert.Contains("Port to listen on when in server mode", output);
        }

        [Fact]
        public void Should_Start_Server_With_Correct_Output()
        {
            // Arrange & Act
            var process = new Process
            {
                StartInfo = new ProcessStartInfo
                {
                    FileName = "dotnet",
                    Arguments = $"run --project {GetCliProjectPath()} -- --server --port 0 --db {_tempDbFile}",
                    UseShellExecute = false,
                    RedirectStandardOutput = true,
                    RedirectStandardError = true
                }
            };

            process.Start();
            
            // Wait a short time to capture startup output
            var outputBuffer = "";
            var startTime = DateTime.Now;
            
            while ((DateTime.Now - startTime).TotalSeconds < 3)
            {
                if (!process.StandardOutput.EndOfStream)
                {
                    outputBuffer += process.StandardOutput.ReadLine() + "\n";
                }
                
                if (outputBuffer.Contains("LiNo WebSocket server started"))
                {
                    break;
                }
                
                Thread.Sleep(100);
            }
            
            process.Kill();
            process.WaitForExit();

            // Assert
            Assert.Contains("LiNo WebSocket server started", outputBuffer);
            Assert.Contains("Press Ctrl+C to stop the server", outputBuffer);
        }

        [Fact]
        public void Should_Preserve_Normal_CLI_Functionality()
        {
            // Arrange & Act
            var process = new Process
            {
                StartInfo = new ProcessStartInfo
                {
                    FileName = "dotnet",
                    Arguments = $"run --project {GetCliProjectPath()} -- --db {_tempDbFile} '() ((1 1))' --changes --after",
                    UseShellExecute = false,
                    RedirectStandardOutput = true,
                    RedirectStandardError = true
                }
            };

            process.Start();
            var output = process.StandardOutput.ReadToEnd();
            process.WaitForExit();

            // Assert - Normal CLI functionality should work
            Assert.Equal(0, process.ExitCode);
            Assert.Contains("(1: 1 1)", output); // Should show created link
        }

        private string GetCliProjectPath()
        {
            // Get the current test directory and navigate to the CLI project
            var currentDir = Directory.GetCurrentDirectory();
            var projectRoot = Directory.GetParent(currentDir)?.Parent?.Parent?.FullName;
            return Path.Combine(projectRoot ?? currentDir, "Foundation.Data.Doublets.Cli");
        }
    }
}