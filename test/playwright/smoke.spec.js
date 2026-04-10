import { test, expect } from "@playwright/test";

test("home renders app title in shell", async ({ page }) => {
  await page.goto("/");
  await expect(page.locator(".brand strong")).toHaveText("Local AI Worker");
});

test("workers view opens from nav", async ({ page }) => {
  await page.goto("/");
  await page.getByRole("button", { name: "Workers" }).click();
  await expect(page.getByRole("heading", { name: "Workers" })).toBeVisible();
  await expect(page.getByRole("button", { name: "Add worker" })).toBeVisible();
});
