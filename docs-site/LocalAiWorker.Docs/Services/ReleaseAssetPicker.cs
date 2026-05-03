namespace LocalAiWorker.Docs.Services;

public enum DownloadOsKind
{
    Windows,
    MacOsAppleSilicon,
    MacOsIntel,
    LinuxAppImage,
    LinuxDeb,
    Unknown,
}

public sealed record OsDownloadLinks(
    string? Windows,
    string? MacAppleSilicon,
    string? MacIntel,
    string? LinuxAppImage,
    string? LinuxDeb,
    string? Fallback);

public sealed class ReleaseAssetPicker
{
    public OsDownloadLinks Pick(IReadOnlyList<GitHubAssetDto> assets)
    {
        string? win = null, macArm = null, macX64 = null, appImage = null, deb = null, fallback = null;

        foreach (var a in assets)
        {
            var n = a.Name.ToLowerInvariant();
            var url = a.BrowserDownloadUrl;
            if (string.IsNullOrEmpty(url))
            {
                continue;
            }

            if (n.EndsWith(".msi", StringComparison.Ordinal))
            {
                win = url;
            }
            else if (n.EndsWith(".dmg", StringComparison.Ordinal))
            {
                if (n.Contains("aarch64", StringComparison.Ordinal) || n.Contains("arm64", StringComparison.Ordinal))
                {
                    macArm = url;
                }
                else if (n.Contains("x64", StringComparison.Ordinal) || n.Contains("x86_64", StringComparison.Ordinal) || n.Contains("intel", StringComparison.Ordinal))
                {
                    macX64 = url;
                }
                else
                {
                    macArm ??= url;
                }
            }
            else if (n.EndsWith(".appimage", StringComparison.Ordinal))
            {
                appImage = url;
            }
            else if (n.EndsWith(".deb", StringComparison.Ordinal))
            {
                deb = url;
            }

            fallback ??= url;
        }

        return new OsDownloadLinks(win, macArm, macX64, appImage, deb, fallback);
    }

    public DownloadOsKind DetectFromUserAgent(string? userAgent)
    {
        if (string.IsNullOrEmpty(userAgent))
        {
            return DownloadOsKind.Unknown;
        }

        var ua = userAgent.ToLowerInvariant();
        if (ua.Contains("windows", StringComparison.Ordinal))
        {
            return DownloadOsKind.Windows;
        }

        if (ua.Contains("mac os x", StringComparison.Ordinal) || ua.Contains("macintosh", StringComparison.Ordinal))
        {
            return ua.Contains("arm", StringComparison.Ordinal) || ua.Contains("aarch64", StringComparison.Ordinal)
                ? DownloadOsKind.MacOsAppleSilicon
                : DownloadOsKind.MacOsIntel;
        }

        if (ua.Contains("linux", StringComparison.Ordinal))
        {
            return ua.Contains("ubuntu", StringComparison.Ordinal) || ua.Contains("debian", StringComparison.Ordinal)
                ? DownloadOsKind.LinuxDeb
                : DownloadOsKind.LinuxAppImage;
        }

        return DownloadOsKind.Unknown;
    }

    public string? PickDefaultUrl(OsDownloadLinks links, DownloadOsKind kind) =>
        kind switch
        {
            DownloadOsKind.Windows => links.Windows ?? links.Fallback,
            DownloadOsKind.MacOsAppleSilicon => links.MacAppleSilicon ?? links.MacIntel ?? links.Fallback,
            DownloadOsKind.MacOsIntel => links.MacIntel ?? links.MacAppleSilicon ?? links.Fallback,
            DownloadOsKind.LinuxDeb => links.LinuxDeb ?? links.LinuxAppImage ?? links.Fallback,
            DownloadOsKind.LinuxAppImage => links.LinuxAppImage ?? links.LinuxDeb ?? links.Fallback,
            _ => links.Windows ?? links.MacAppleSilicon ?? links.MacIntel ?? links.LinuxAppImage ?? links.LinuxDeb ?? links.Fallback,
        };
}
