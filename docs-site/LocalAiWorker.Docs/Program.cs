using LocalAiWorker.Docs;
using LocalAiWorker.Docs.Services;

// Use build output as content root so linked wwwroot assets (e.g. docs/images → wwwroot/images)
// resolve under dotnet run as well as publish/Docker (assembly dir already contains appsettings + docs/*.md).
var builder = WebApplication.CreateBuilder(new WebApplicationOptions
{
    Args = args,
    ContentRootPath = AppContext.BaseDirectory,
});

builder.Services.AddRazorPages();
builder.Services.AddMemoryCache();
builder.Services.Configure<M43Options>(builder.Configuration.GetSection(M43Options.SectionName));
builder.Services.Configure<GitHubOptions>(builder.Configuration.GetSection(GitHubOptions.SectionName));
builder.Services.AddHttpClient<GitHubReleaseClient>((sp, client) =>
{
    client.DefaultRequestHeaders.UserAgent.ParseAdd("LocalAiWorker-Docs/1.0");
    client.DefaultRequestHeaders.Accept.ParseAdd("application/vnd.github+json");
});
builder.Services.AddSingleton<ReleaseAssetPicker>();
builder.Services.AddSingleton<DocsMarkdownRenderer>();

var app = builder.Build();

if (!app.Environment.IsDevelopment())
{
    app.UseExceptionHandler("/Error");
}

app.UseStatusCodePagesWithReExecute("/Error", "?code={0}");
app.UseDefaultFiles();
app.UseStaticFiles();
app.MapGet("/health", () => Results.Json(new { status = "ok" }));
app.MapRazorPages();
app.Run();

// For WebApplicationFactory in tests
public partial class Program;
