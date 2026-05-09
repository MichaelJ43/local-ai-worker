using Microsoft.Playwright;
using NUnit.Framework;

namespace LocalAiWorker.Docs.UiTests;

[TestFixture]
public sealed class SmokeTests
{
    private IPlaywright _playwright = null!;
    private IBrowser _browser = null!;

    private static string BaseUrl =>
        Environment.GetEnvironmentVariable("DOCS_BASE_URL") ?? "http://127.0.0.1:5055";

    [OneTimeSetUp]
    public async Task OneTimeSetUpAsync()
    {
        _playwright = await Playwright.CreateAsync();
        _browser = await _playwright.Chromium.LaunchAsync(new BrowserTypeLaunchOptions { Headless = true });
    }

    [OneTimeTearDown]
    public void OneTimeTearDown()
    {
        _browser.CloseAsync().GetAwaiter().GetResult();
        _playwright.Dispose();
    }

    [Test]
    public async Task Home_shows_heading()
    {
        var page = await _browser.NewPageAsync();
        try
        {
            await page.GotoAsync(BaseUrl.TrimEnd('/') + "/");
            var h1 = page.Locator("h1").First;
            StringAssert.Contains("Local AI Worker", await h1.InnerTextAsync());
        }
        finally
        {
            await page.CloseAsync();
        }
    }

    [Test]
    public async Task Guide_contains_title_heading()
    {
        var page = await _browser.NewPageAsync();
        try
        {
            await page.GotoAsync(BaseUrl.TrimEnd('/') + "/Guide");
            var body = await page.Locator("body").InnerTextAsync();
            StringAssert.Contains("Local AI Worker", body);
            StringAssert.Contains("User guide", body);
        }
        finally
        {
            await page.CloseAsync();
        }
    }

    [Test]
    public async Task GettingStarted_loads()
    {
        var page = await _browser.NewPageAsync();
        try
        {
            await page.GotoAsync(BaseUrl.TrimEnd('/') + "/GettingStarted");
            var body = await page.Locator("body").InnerTextAsync();
            StringAssert.Contains("Getting started", body);
            StringAssert.Contains("Docker Desktop", body);
        }
        finally
        {
            await page.CloseAsync();
        }
    }

    [Test]
    public async Task Health_returns_json()
    {
        var page = await _browser.NewPageAsync();
        try
        {
            var response = await page.APIRequest.GetAsync(BaseUrl.TrimEnd('/') + "/health");
            Assert.That(response.Ok, Is.True);
            var json = await response.JsonAsync();
            Assert.That(json, Is.Not.Null);
        }
        finally
        {
            await page.CloseAsync();
        }
    }
}
