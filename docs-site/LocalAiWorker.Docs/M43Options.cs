namespace LocalAiWorker.Docs;

public sealed class M43Options
{
    public const string SectionName = "M43";

    /// <summary>Origin for static-assets CDN (no trailing slash); e.g. https://static.michaelj43.dev</summary>
    public string StaticAssetsBaseUrl { get; set; } = "";

    public string ApiBaseUrl { get; set; } = "https://api.michaelj43.dev";

    public string AuthOrigin { get; set; } = "https://auth.michaelj43.dev";

    public string BaseForV1 => StaticAssetsBaseUrl.TrimEnd('/');
}
