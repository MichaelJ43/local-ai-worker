using LocalAiWorker.Docs;
using LocalAiWorker.Docs.Services;
var builder = WebApplication.CreateBuilder(args);

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
