namespace LocalAiWorker.Docs;

public sealed class GitHubOptions
{
    public const string SectionName = "GitHub";

    /// <summary>owner/name for releases API</summary>
    public string Repository { get; set; } = "MichaelJ43/local-ai-worker";
}
