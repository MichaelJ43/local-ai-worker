using LocalAiWorker.Docs.Services;
using Xunit;

namespace LocalAiWorker.Docs.Tests;

public class DocsMarkdownRendererTests
{
    [Fact]
    public void Preprocess_rewrite_image_links_and_user_guide()
    {
        const string md = """
            ![x](images/foo.png)
            [USER_GUIDE.md](USER_GUIDE.md#process-overview)
            """;

        var result = DocsMarkdownRenderer.PreprocessMarkdown(md);

        Assert.Contains("](/images/foo.png)", result);
        Assert.Contains("[User guide](/Guide#process-overview)", result);
        Assert.DoesNotContain("USER_GUIDE.md", result);
    }

    [Fact]
    public void Preprocess_repo_sandbox_callout_becomes_internal_link()
    {
        const string md = "See **`docs/REPO_AGENT_SANDBOX.md`** for sandbox policy.";
        var result = DocsMarkdownRenderer.PreprocessMarkdown(md);
        Assert.Contains("[Repo agent sandbox](/RepoAgentSandbox)", result);
    }

    [Fact]
    public void Render_sample_includes_mermaid_div_not_pre_code()
    {
        const string md = """
            ```mermaid
            flowchart LR
              A-->B
            ```
            """;
        var html = DocsMarkdownRenderer.RenderMarkdownToHtml(md);
        Assert.Contains("class=\"mermaid\"", html);
        Assert.DoesNotContain("language-mermaid", html);
        Assert.Contains("flowchart LR", html);
    }

}
