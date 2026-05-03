using LocalAiWorker.Docs.Services;
using Microsoft.AspNetCore.Mvc.RazorPages;

namespace LocalAiWorker.Docs.Pages;

public class DownloadModel : PageModel
{
    private readonly GitHubReleaseClient _releases;
    private readonly ReleaseAssetPicker _picker;

    public DownloadModel(GitHubReleaseClient releases, ReleaseAssetPicker picker)
    {
        _releases = releases;
        _picker = picker;
    }

    public string? ReleaseTag { get; private set; }
    public OsDownloadLinks Links { get; private set; } = new(null, null, null, null, null, null);
    public string? SuggestedUrl { get; private set; }

    public async Task OnGetAsync(CancellationToken cancellationToken)
    {
        var rel = await _releases.GetLatestAsync(cancellationToken);
        if (rel?.TagName is null || rel.Assets.Count == 0)
        {
            return;
        }

        ReleaseTag = rel.TagName;
        Links = _picker.Pick(rel.Assets);
        var ua = Request.Headers.UserAgent.ToString();
        var kind = _picker.DetectFromUserAgent(ua);
        SuggestedUrl = _picker.PickDefaultUrl(Links, kind);
    }
}
