using LocalAiWorker.Docs.Services;
using Xunit;

namespace LocalAiWorker.Docs.Tests;

public class ReleaseAssetPickerTests
{
    private readonly ReleaseAssetPicker _sut = new();

    [Fact]
    public void Pick_maps_msi_windows_dmg_arm_and_appimage()
    {
        var assets = new List<GitHubAssetDto>
        {
            new() { Name = "app-0.1.0_x64_en-US.msi", BrowserDownloadUrl = "https://example/msi" },
            new() { Name = "app_0.1.0_aarch64.dmg", BrowserDownloadUrl = "https://example/dmg-arm" },
            new() { Name = "app_0.1.0_amd64.AppImage", BrowserDownloadUrl = "https://example/ai" },
            new() { Name = "app_0.1.0_amd64.deb", BrowserDownloadUrl = "https://example/deb" },
        };

        var links = _sut.Pick(assets);

        Assert.Equal("https://example/msi", links.Windows);
        Assert.Equal("https://example/dmg-arm", links.MacAppleSilicon);
        Assert.Equal("https://example/ai", links.LinuxAppImage);
        Assert.Equal("https://example/deb", links.LinuxDeb);
    }

    [Theory]
    [InlineData("Mozilla/5.0 (Windows NT 10.0; Win64; x64)", DownloadOsKind.Windows)]
    [InlineData("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7)", DownloadOsKind.MacOsIntel)]
    [InlineData("Mozilla/5.0 (Macintosh; ARM Mac OS X 14_0)", DownloadOsKind.MacOsAppleSilicon)]
    public void DetectFromUserAgent_classifies(string ua, DownloadOsKind expected) =>
        Assert.Equal(expected, _sut.DetectFromUserAgent(ua));
}
