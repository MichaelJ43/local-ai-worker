using LocalAiWorker.Docs.Services;
using Microsoft.AspNetCore.Mvc.RazorPages;

namespace LocalAiWorker.Docs.Pages;

public class ArchitectureModel : PageModel
{
    private readonly DocsMarkdownRenderer _renderer;

    public ArchitectureModel(DocsMarkdownRenderer renderer) => _renderer = renderer;

    public string HtmlContent { get; private set; } = "";

    public void OnGet() => HtmlContent = _renderer.Render("architecture.md").Html;
}
