using System.Net;
using System.Text.Json;
using Microsoft.Extensions.Caching.Memory;
using Microsoft.Extensions.Options;

namespace LocalAiWorker.Docs.Services;

public sealed class GitHubReleaseClient
{
    private static readonly JsonSerializerOptions JsonOptions = new() { PropertyNameCaseInsensitive = true };

    private readonly HttpClient _http;
    private readonly IMemoryCache _cache;
    private readonly GitHubOptions _options;

    public GitHubReleaseClient(HttpClient http, IMemoryCache cache, IOptions<GitHubOptions> options)
    {
        _http = http;
        _cache = cache;
        _options = options.Value;
    }

    public async Task<GitHubReleaseDto?> GetLatestAsync(CancellationToken ct = default)
    {
        var repo = _options.Repository.Trim();
        var cacheKey = "gh:latest:" + repo;
        if (_cache.TryGetValue(cacheKey, out GitHubReleaseDto? cached))
        {
            return cached;
        }

        var url = $"https://api.github.com/repos/{repo}/releases/latest";
        using var response = await _http.GetAsync(url, HttpCompletionOption.ResponseHeadersRead, ct);
        if (response.StatusCode == HttpStatusCode.NotFound)
        {
            return null;
        }

        response.EnsureSuccessStatusCode();
        await using var stream = await response.Content.ReadAsStreamAsync(ct);
        var dto = await JsonSerializer.DeserializeAsync<GitHubReleaseDto>(stream, JsonOptions, ct);
        if (dto != null)
        {
            _cache.Set(cacheKey, dto, TimeSpan.FromMinutes(5));
        }

        return dto;
    }
}
