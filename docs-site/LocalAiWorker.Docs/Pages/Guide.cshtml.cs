using LocalAiWorker.Docs.Services;
using Microsoft.AspNetCore.Mvc.RazorPages;

namespace LocalAiWorker.Docs.Pages;

public class GuideModel : PageModel
{
    private readonly DocsMarkdownRenderer _renderer;

    public GuideModel(DocsMarkdownRenderer renderer) => _renderer = renderer;

    public string HtmlContent { get; private set; } = "";

    public void OnGet() => HtmlContent = _renderer.Render("USER_GUIDE.md").Html;
}
