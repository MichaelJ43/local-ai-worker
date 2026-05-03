using Microsoft.AspNetCore.Mvc;
using Microsoft.AspNetCore.Mvc.RazorPages;

namespace LocalAiWorker.Docs.Pages;

public class ErrorModel : PageModel
{
    public string? Message { get; private set; }

    public void OnGet([FromQuery] int? code)
    {
        Message = code is >= 400 and < 600 ? $"HTTP {code}" : null;
    }
}
