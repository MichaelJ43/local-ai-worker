using LocalAiWorker.Docs.Services;
using Microsoft.AspNetCore.Mvc.RazorPages;

namespace LocalAiWorker.Docs.Pages;

public class RepoAgentSandboxModel : PageModel
{
    private readonly DocsMarkdownRenderer _renderer;

    public RepoAgentSandboxModel(DocsMarkdownRenderer renderer) => _renderer = renderer;

    public string HtmlContent { get; private set; } = "";

    public void OnGet() => HtmlContent = _renderer.Render("REPO_AGENT_SANDBOX.md").Html;
}
