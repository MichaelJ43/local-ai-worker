using System.Net;
using System.Text.RegularExpressions;
using Markdig;

namespace LocalAiWorker.Docs.Services;

/// <summary>
/// Renders bundled repo markdown (copied next to the app under <c>docs/</c>) to HTML with Mermaid support.
/// </summary>
public sealed class DocsMarkdownRenderer
{
    private static readonly MarkdownPipeline Pipeline = new MarkdownPipelineBuilder()
        .UseAdvancedExtensions()
        .Build();

    /// <summary>
    /// Resolves <c>docs/&lt;fileName&gt;</c> next to the published assembly (same rule as <see cref="ResolveDocsPath"/>).
    /// </summary>
    public DocsMarkdownResult Render(string fileName)
    {
        var path = ResolveDocsPath(fileName);
        var markdown = File.ReadAllText(path);
        var html = RenderMarkdownToHtml(markdown);
        return new DocsMarkdownResult(html);
    }

    public static string ResolveDocsPath(string fileName)
    {
        var baseDir = AppContext.BaseDirectory;
        var path = Path.Combine(baseDir, "docs", fileName);
        if (!File.Exists(path))
            throw new FileNotFoundException($"Bundled doc not found: {path}", path);
        return path;
    }

    internal static string PreprocessMarkdown(string markdown)
    {
        var s = markdown;

        // Root-relative image and intra-doc links so they work from any route (e.g. /Guide).
        s = s.Replace("](images/", "](/images/", StringComparison.Ordinal);

        s = Regex.Replace(
            s,
            @"\]\(USER_GUIDE\.md(#[^)]*)?\)",
            m => "](" + "/Guide" + m.Groups[1].Value + ")",
            RegexOptions.None);

        s = s.Replace("[USER_GUIDE.md](/Guide", "[User guide](/Guide", StringComparison.Ordinal);

        s = s.Replace(
            "See **`docs/REPO_AGENT_SANDBOX.md`** for",
            "See [Repo agent sandbox](/RepoAgentSandbox) for",
            StringComparison.Ordinal);

        return s;
    }

    internal static string RenderMarkdownToHtml(string markdown)
    {
        var preprocessed = PreprocessMarkdown(markdown);
        var html = Markdown.ToHtml(preprocessed, Pipeline);
        html = TransformMermaidCodeBlocks(html);
        return html;
    }

    private static string TransformMermaidCodeBlocks(string html)
    {
        return Regex.Replace(
            html,
            @"<pre><code class=""language-mermaid"">([\s\S]*?)</code></pre>",
            m =>
            {
                var inner = WebUtility.HtmlDecode(m.Groups[1].Value).TrimEnd();
                return "<div class=\"mermaid\">" + inner + "</div>";
            },
            RegexOptions.None);
    }
}

public readonly record struct DocsMarkdownResult(string Html);
